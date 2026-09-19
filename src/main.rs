mod comin;
mod logs;
mod model;
mod notifications;
mod tray;

use std::time::Duration;

use anyhow::Result;
use comin::CominClient;
use ksni::TrayMethods;
use model::{Phase, TrayState};
use tokio::{sync::mpsc, time::MissedTickBehavior};
use tray::{Action, CominTray};

#[tokio::main]
async fn main() -> Result<()> {
    let client = CominClient::default();
    let initial_state = read_state(&client).await;
    let (action_tx, mut action_rx) = mpsc::channel(8);
    let tray = CominTray {
        state: initial_state.clone(),
        action_tx,
    };
    let tray_handle = tray.spawn().await?;

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
                if matches!(action, Action::Quit) {
                    break;
                }
                run_action(&client, action).await;
                update_state(&client, &tray_handle, &mut state).await;
            }
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

async fn run_action(client: &CominClient, action: Action) {
    let result = match action {
        Action::Fetch => client.fetch().await,
        Action::Suspend => client.suspend().await,
        Action::Resume => client.resume().await,
        Action::SwitchLatest => client.switch_latest().await,
        Action::OpenLogs => logs::open(),
        Action::Quit => return,
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
