mod comin;
mod log_viewer;
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
use tray::{Action, CominTray};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    if matches!(
        args.next().as_deref(),
        Some("logs" | "log-viewer" | "--logs")
    ) {
        log_viewer::run()?;
        return Ok(());
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
    let mut notified_reboot = None;
    let mut refresh = tokio::time::interval(Duration::from_secs(3));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    refresh.tick().await;

    let mut log_viewer_child: Option<std::process::Child> = None;

    loop {
        tokio::select! {
            _ = refresh.tick() => {
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
            Some(action) = action_rx.recv() => {
                run_action(&client, action, &mut log_viewer_child).await;
                update_state(&client, &tray_handle, &mut state, &mut notified_reboot).await;
            }
        }
    }
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

/// Comin re-checks its remote on an interval, which briefly reports fetching
/// or evaluating even with nothing to do. That flips `Phase` away from
/// `RebootRequired` and back, so a plain phase-transition notification would
/// resend every cycle. Key this one on the deployment's identity instead, so
/// it only fires once per deployment that actually needs a restart.
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
    log_viewer_child: &mut Option<std::process::Child>,
) {
    let result = match action {
        Action::Fetch => client.fetch().await,
        Action::Suspend => client.suspend().await,
        Action::Resume => client.resume().await,
        Action::SwitchLatest => client.switch_latest().await,
        Action::OpenLogs => {
            if let Some(child) = log_viewer_child.as_mut() {
                match child.try_wait() {
                    Ok(None) => return,
                    _ => *log_viewer_child = None,
                }
            }

            match std::env::current_exe() {
                Ok(exe) => match std::process::Command::new(exe).arg("logs").spawn() {
                    Ok(child) => {
                        *log_viewer_child = Some(child);
                        Ok(())
                    }
                    Err(error) => Err(anyhow::anyhow!("Failed to launch log viewer: {error}")),
                },
                Err(error) => Err(anyhow::anyhow!("Failed to get current executable path: {error}")),
            }
        }
        Action::Quit => {
            if let Some(mut child) = log_viewer_child.take() {
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
