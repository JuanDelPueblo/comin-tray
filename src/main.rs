mod comin;
mod log_viewer;
mod logs;
mod model;
mod notifications;
mod tray;

use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::Result;
use comin::CominClient;
use ksni::TrayMethods;
use model::{Phase, TrayState};
use tokio::{runtime::Builder, sync::mpsc, time::MissedTickBehavior};
use tray::{Action, CominTray};

fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread().enable_all().build()?;
    let open_logs = Arc::new(AtomicBool::new(false));

    runtime.block_on(start_tray(Arc::clone(&open_logs)))?;

    // The log window owns the main thread from here on. The tray and its
    // polling loop keep running on the runtime's worker threads.
    log_viewer::run(open_logs)?;

    Ok(())
}

async fn start_tray(open_logs: Arc<AtomicBool>) -> Result<()> {
    let client = CominClient::default();
    let initial_state = read_state(&client).await;
    let (action_tx, mut action_rx) = mpsc::channel(8);
    let tray = CominTray {
        state: initial_state.clone(),
        action_tx,
    };
    let tray_handle = tray.spawn().await?;

    tokio::spawn(async move {
        let mut state = initial_state;
        let mut refresh = tokio::time::interval(Duration::from_secs(3));
        refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
        refresh.tick().await;

        loop {
            tokio::select! {
                _ = refresh.tick() => {
                    update_state(&client, &tray_handle, &mut state).await;
                }
                Some(action) = action_rx.recv() => {
                    run_action(&client, action, &open_logs).await;
                    update_state(&client, &tray_handle, &mut state).await;
                }
            }
        }
    });

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
) {
    let next = read_state(client).await;
    if next.phase != current.phase {
        notify_phase_change(current.phase, &next).await;
    }
    *current = next.clone();
    let _ = tray_handle
        .update(move |tray| {
            tray.state = next;
        })
        .await;
}

async fn run_action(client: &CominClient, action: Action, open_logs: &Arc<AtomicBool>) {
    let result = match action {
        Action::Fetch => client.fetch().await,
        Action::Suspend => client.suspend().await,
        Action::Resume => client.resume().await,
        Action::SwitchLatest => client.switch_latest().await,
        Action::OpenLogs => {
            open_logs.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        Action::Quit => std::process::exit(0),
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
        Phase::RebootRequired => (
            "Comin",
            "Restart the computer to use the latest deployment.".into(),
        ),
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
