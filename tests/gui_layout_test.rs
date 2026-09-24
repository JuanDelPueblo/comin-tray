use comin_tray::gui::{
    deployments::{DeploymentsMessage, DeploymentsView},
    log_list::LogListMessage,
    log_view::{LogMessage, LogView},
    overview,
};
use comin_tray::logs::{LogEntry, Priority, parse_log_line};
use comin_tray::model::CominState;

const FIXTURES: [&str; 5] = [
    include_str!("fixtures/status_normal.json"),
    include_str!("fixtures/status_evaluating.json"),
    include_str!("fixtures/status_failed_eval.json"),
    include_str!("fixtures/status_suspended.json"),
    include_str!("fixtures/status_confirmation_needed.json"),
];

fn journal() -> Vec<LogEntry> {
    include_str!("fixtures/journal_boot_deploy.jsonl")
        .lines()
        .map(|line| parse_log_line(line).expect("valid journal fixture").0)
        .collect()
}

#[test]
fn test_overview_view_does_not_panic() {
    // Test with None state
    let _ = overview::view(None, None, None, None);

    // Test with each fixture
    for json in FIXTURES {
        let state: CominState = serde_json::from_str(json).expect("valid fixture json");
        let _ = overview::view(Some(&state), None, None, None);
        let _ = overview::view(Some(&state), Some("Evaluating..."), None, None);
        let _ = overview::view(Some(&state), None, Some("Error occurred"), None);
    }
}

#[test]
fn test_log_view_does_not_panic() {
    let mut log_view = LogView::default();
    let _ = log_view.view();
    for entry in journal() {
        let _ = log_view.update(LogMessage::LogReceived(entry));
    }
    let _ = log_view.update(LogMessage::LogReceived(LogEntry::note(
        Priority::Error,
        "error message",
    )));
    let _ = log_view.view();

    // Select a range and copy it.
    let _ = log_view.update(LogMessage::List(LogListMessage::RowPressed(3)));
    let _ = log_view.update(LogMessage::CopySelected);
    let _ = log_view.update(LogMessage::HideNoise(false));
    let _ = log_view.update(LogMessage::Search("building".into()));
    let _ = log_view.view();
}

#[test]
fn test_deployments_view_does_not_panic() {
    let _ = DeploymentsView::default().view(None);

    for json in FIXTURES {
        let state: CominState = serde_json::from_str(json).expect("valid fixture json");
        let mut view = DeploymentsView::default();
        let _ = view.view(Some(&state));

        // Selecting the newest item starts loading its log.
        let _ = view.sync(&state);
        let _ = view.view(Some(&state));

        let Some(item) = state.history().into_iter().next() else {
            continue;
        };
        let Some((since, _)) = item.log_window() else {
            continue;
        };
        let _ = view.update(
            DeploymentsMessage::Loaded {
                key: item.key.clone(),
                live: item.active,
                since,
                result: Ok(journal()),
            },
            Some(&state),
        );
        let _ = view.update(DeploymentsMessage::ToggleDerivations, Some(&state));
        let _ = view.update(
            DeploymentsMessage::List(LogListMessage::RowPressed(1)),
            Some(&state),
        );
        let _ = view.view(Some(&state));
        assert!(!view.selected_text().is_empty());

        let _ = view.update(
            DeploymentsMessage::Stage(Some(comin_tray::build_log::Stage::Build)),
            Some(&state),
        );
        let _ = view.view(Some(&state));
    }
}

#[test]
fn test_deployment_history_lists_undeployed_generations() {
    let state: CominState = serde_json::from_str(FIXTURES[0]).expect("valid fixture json");
    let history = state.history();

    // The deployed generation and the newer, already-built one that was
    // never deployed because its output was identical.
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status(), "not deployed");
    assert_eq!(history[1].status(), "done");
    assert_eq!(history[1].operation(), Some("boot"));

    let evaluating: CominState = serde_json::from_str(FIXTURES[1]).expect("valid fixture json");
    let history = evaluating.history();
    assert_eq!(history.len(), 1);
    assert!(history[0].active);
    assert_eq!(history[0].status(), "evaluating");
    assert_eq!(history[0].log_window().unwrap().1, None);

    let failed: CominState = serde_json::from_str(FIXTURES[2]).expect("valid fixture json");
    assert_eq!(failed.history()[0].status(), "failed");
}
