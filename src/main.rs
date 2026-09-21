mod comin;
mod format;
mod gui;
mod logs;
mod model;
mod notifications;
mod tray;

use std::time::Duration;

use anyhow::Result;
use comin::CominClient;
use ksni::TrayMethods;
use model::{Phase, TrayState};
use tokio::{runtime::Builder, sync::mpsc, time::MissedTickBehavior};
use tray::{Action, CominTray, GuiPage};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = args.first().map(String::as_str);

    match subcommand {
        Some("gui") => {
            let page = if args.get(1).map(String::as_str) == Some("logs") {
                GuiPage::Logs
            } else {
                GuiPage::Overview
            };
            gui::run(page)?;
            return Ok(());
        }
        Some("logs" | "log-viewer" | "--logs") => {
            gui::run(GuiPage::Logs)?;
            return Ok(());
        }
        Some("--help" | "-h" | "help") => {
            println!("Comin GitOps Manager & System Tray\n");
            println!("Usage: comin-tray [COMMAND]\n");
            println!("Commands:");
            println!(
                "  gui [logs]   Launch the native Comin GUI dashboard (default: Overview; 'logs' opens Logs tab)"
            );
            println!("  logs         Launch the Comin GUI directly on the Logs tab");
            println!(
                "  tray         Run the persistent system tray daemon (default when run without arguments)"
            );
            println!("  --help, -h   Print this help message");
            return Ok(());
        }
        Some("tray") | None => {}
        Some(unknown) => {
            eprintln!("Unknown argument: {unknown}");
            eprintln!("Usage: comin-tray [gui [logs] | logs | tray | --help]");
            std::process::exit(1);
        }
    }

    let runtime = Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_tray())?;

    Ok(())
}

async fn run_tray() -> Result<()> {
    let client = CominClient::default();
    let initial_state = read_state(&client).await;
    let (action_tx, mut action_rx) = mpsc::channel(8);
    let tray = CominTray {
        state: initial_state.clone(),
        action_tx,
    };
    let tray_handle = tray.spawn().await?;

    let mut state = initial_state;
    let mut notified_reboot: Option<String> = None;
    let mut refresh = tokio::time::interval(Duration::from_secs(3));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    refresh.tick().await;

    let mut gui_child: Option<std::process::Child> = None;

    loop {
        tokio::select! {
            _ = refresh.tick() => {
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
            Some(action) = action_rx.recv() => {
                run_action(&client, action, &mut gui_child).await;
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
            else => break,
        }
    }

    Ok(())
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
    gui_child: &mut Option<std::process::Child>,
) {
    let result = match action {
        Action::Fetch => client.fetch().await,
        Action::Suspend => client.suspend().await,
        Action::Resume => client.resume().await,
        Action::SwitchLatest => client.switch_latest().await,
        Action::RetryLatest => client.retry_latest().await,
        Action::AcceptConfirmation => client.accept_confirmation().await,
        Action::OpenGui(page) => {
            if let Some(child) = gui_child.as_mut() {
                match child.try_wait() {
                    Ok(None) => return,
                    _ => *gui_child = None,
                }
            }

            match std::env::current_exe() {
                Ok(exe) => {
                    let mut cmd = std::process::Command::new(exe);
                    cmd.arg("gui");
                    if page == GuiPage::Logs {
                        cmd.arg("logs");
                    }
                    match cmd.spawn() {
                        Ok(child) => {
                            *gui_child = Some(child);
                            Ok(())
                        }
                        Err(error) => Err(anyhow::anyhow!("Failed to launch GUI: {error}")),
                    }
                }
                Err(error) => Err(anyhow::anyhow!(
                    "Failed to get current executable path: {error}"
                )),
            }
        }
        Action::Quit => {
            if let Some(mut child) = gui_child.take() {
                let _ = child.kill();
            }
            std::process::exit(0);
        }
    };

    if let Err(error) = result {
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
