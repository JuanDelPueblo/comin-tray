use std::time::Duration;

use anyhow::{Context, Result};
use comin_tray::{
    comin::CominClient,
    dbus, format, gui, logs,
    model::{Phase, TrayState},
    notifications,
    tray::{Action, CominTray, GuiPage},
};
use ksni::TrayMethods;
use tokio::{runtime::Builder, sync::mpsc, time::MissedTickBehavior};

fn main() -> Result<()> {
    // Must run before any thread starts; see `format::init_local_offset`.
    format::init_local_offset();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = args.first().map(String::as_str);

    match subcommand {
        Some("gui") => {
            let page = match args.get(1) {
                Some(name) => match GuiPage::parse(name) {
                    Some(page) => page,
                    None => {
                        eprintln!("Unknown page: {name} (expected overview, deployments or logs)");
                        std::process::exit(1);
                    }
                },
                None => GuiPage::Overview,
            };
            gui::run(page)?;
            return Ok(());
        }
        Some("logs" | "log-viewer" | "--logs") => {
            gui::run(GuiPage::Logs)?;
            return Ok(());
        }
        Some("deployments") => {
            gui::run(GuiPage::Deployments)?;
            return Ok(());
        }
        Some("diagnose") => {
            let runtime = Builder::new_multi_thread().enable_all().build()?;
            let healthy = runtime.block_on(diagnose());
            std::process::exit(if healthy { 0 } else { 1 });
        }
        Some("--help" | "-h" | "help") => {
            println!("Comin GitOps Manager & System Tray\n");
            println!("Usage: comin-tray [COMMAND]\n");
            println!("Commands:");
            println!(
                "  gui [PAGE]   Open the Comin window on PAGE: overview (default), deployments or logs"
            );
            println!("  deployments  Open the Comin window on the Deployments page");
            println!("  logs         Open the Comin window on the Logs page");
            println!(
                "  tray         Run the persistent system tray daemon (default when run without arguments)"
            );
            println!("  diagnose     Check why the tray icon, logs or status might not work");
            println!("  --help, -h   Print this help message");
            return Ok(());
        }
        Some("tray") | None => {}
        Some(unknown) => {
            eprintln!("Unknown argument: {unknown}");
            eprintln!(
                "Usage: comin-tray [gui [overview|deployments|logs] | deployments | logs | tray | diagnose | --help]"
            );
            std::process::exit(1);
        }
    }

    let runtime = Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_tray())?;

    Ok(())
}

async fn run_tray() -> Result<()> {
    // Hold the well-known name for the lifetime of the daemon so that a
    // second autostart entry or user service does not add a duplicate icon.
    let _instance = match dbus::claim_tray_name().await {
        Ok(Some(connection)) => Some(connection),
        Ok(None) => {
            eprintln!("comin-tray: another tray is already running; exiting");
            return Ok(());
        }
        Err(error) => {
            eprintln!("comin-tray: {error:#}; continuing without the single-instance check");
            None
        }
    };

    let client = CominClient::default();
    let initial_state = read_state(&client).await;
    let (action_tx, mut action_rx) = mpsc::channel(8);
    let tray = CominTray {
        state: initial_state.clone(),
        action_tx,
    };

    // XDG autostart can start the tray before Plasma's StatusNotifierWatcher
    // is on the bus. Without `assume_sni_available`, `spawn` fails in that
    // case and the icon never appears. With it, ksni waits for the watcher.
    let tray_handle = tray
        .assume_sni_available(true)
        .spawn()
        .await
        .context("Could not start the tray icon service")?;
    eprintln!("comin-tray: tray service started");

    let mut state = initial_state;
    let mut notified_reboot: Option<String> = None;
    let mut refresh = tokio::time::interval(Duration::from_secs(3));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    refresh.tick().await;

    let mut gui_children: Vec<std::process::Child> = Vec::new();

    loop {
        tokio::select! {
            _ = refresh.tick() => {
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
            Some(action) = action_rx.recv() => {
                run_action(&client, action, &mut gui_children).await;
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
            else => break,
        }
    }

    Ok(())
}

async fn diagnose() -> bool {
    let mut healthy = true;
    let mut print = |ok: bool, label: &str, detail: &str| {
        healthy &= ok;
        let mark = if ok { "ok  " } else { "FAIL" };
        println!("[{mark}] {label}: {detail}");
    };

    for check in dbus::session_checks().await {
        print(check.ok, &check.label, &check.detail);
    }

    match CominClient::default().status().await {
        Ok(status) => print(
            true,
            "comin status",
            &format!(
                "{} ({})",
                status.hostname().unwrap_or("unknown host"),
                status.phase().label()
            ),
        ),
        Err(error) => print(false, "comin status", &format!("{error:#}")),
    }

    match logs::check_journal_access().await {
        Ok(()) => print(true, "Journal access", "comin.service logs are readable"),
        Err(error) => print(false, "Journal access", &error),
    }

    healthy
}

async fn read_state(client: &CominClient) -> TrayState {
    match client.status().await {
        Ok(status) => TrayState::from_status(status),
        Err(error) => TrayState::unavailable(error.to_string()),
    }
}

async fn update_state(
    client: &CominClient,
    tray_handle: &ksni::Handle<CominTray>,
    current: &mut TrayState,
    notified_reboot: &mut Option<String>,
) {
    let next = read_state(client).await;
    if next.phase != current.phase {
        notify_phase_change(current.phase, &next).await;
    }
    notify_reboot_once(&next, notified_reboot).await;
    *current = next.clone();
    let _ = tray_handle
        .update(move |tray| {
            tray.state = next;
        })
        .await;
}

async fn notify_reboot_once(next: &TrayState, notified_reboot: &mut Option<String>) {
    let Some(status) = &next.status else {
        return;
    };

    if !status.need_to_reboot.unwrap_or(false) {
        *notified_reboot = None;
        return;
    }

    let deployment_uuid = status
        .latest_deployment()
        .map(|deployment| &deployment.uuid);
    if notified_reboot.as_deref() != deployment_uuid.map(String::as_str) {
        let message = status
            .reboot_reason()
            .unwrap_or("Restart the computer to use the latest deployment.");
        let _ = notifications::send("Comin", message, Phase::RebootRequired.icon_name()).await;
        *notified_reboot = deployment_uuid.cloned();
    }
}

async fn run_action(
    client: &CominClient,
    action: Action,
    gui_children: &mut Vec<std::process::Child>,
) {
    // Reap GUI processes that have exited so they do not linger as zombies.
    gui_children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));

    let result = match action {
        Action::Fetch => client.fetch().await,
        Action::Suspend => client.suspend().await,
        Action::Resume => client.resume().await,
        Action::SwitchLatest => client.switch_latest().await,
        Action::RetryLatest => client.retry_latest().await,
        Action::AcceptConfirmation => client.accept_confirmation().await,
        Action::OpenGui(page) => {
            // The GUI process enforces a single window itself: a second
            // launch asks the running window to show `page` and exits.
            match std::env::current_exe() {
                Ok(exe) => match std::process::Command::new(exe)
                    .args(["gui", page.as_str()])
                    .spawn()
                {
                    Ok(child) => {
                        gui_children.push(child);
                        Ok(())
                    }
                    Err(error) => Err(anyhow::anyhow!("Failed to launch GUI: {error}")),
                },
                Err(error) => Err(anyhow::anyhow!(
                    "Failed to get current executable path: {error}"
                )),
            }
        }
        Action::Quit => {
            for child in gui_children.iter_mut() {
                let _ = child.kill();
            }
            std::process::exit(0);
        }
    };

    if let Err(error) = result {
        eprintln!("comin-tray: action failed: {error:#}");
        let _ =
            notifications::send("Comin action failed", &error.to_string(), "dialog-error").await;
    }
}

async fn notify_phase_change(previous: Phase, next: &TrayState) {
    let (summary, message) = match next.phase {
        Phase::Evaluating => ("Comin update found", next.evaluation_message()),
        Phase::Idle if previous == Phase::Deploying => ("Comin", "The deployment finished.".into()),
        Phase::Failed => ("Comin", "A Comin operation failed.".into()),
        Phase::Suspended => ("Comin", "Comin is suspended.".into()),
        Phase::Unavailable => (
            "Comin",
            next.error
                .clone()
                .unwrap_or_else(|| "Comin is unavailable.".into()),
        ),
        _ => return,
    };

    let _ = notifications::send(summary, &message, next.phase.icon_name()).await;
}
