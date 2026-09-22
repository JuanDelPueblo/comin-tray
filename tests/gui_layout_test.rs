use comin_tray::gui::{
    log_view::{LogMessage, LogView},
    overview,
};
use comin_tray::logs::{LogEntry, Priority};
use comin_tray::model::CominState;

#[test]
fn test_overview_view_does_not_panic() {
    let fixtures = [
        include_str!("fixtures/status_normal.json"),
        include_str!("fixtures/status_evaluating.json"),
        include_str!("fixtures/status_failed_eval.json"),
        include_str!("fixtures/status_suspended.json"),
        include_str!("fixtures/status_confirmation_needed.json"),
    ];

    // Test with None state
    let _ = overview::view(None, None, None, None);

    // Test with each fixture
    for json in fixtures {
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
    let _ = log_view.update(LogMessage::LogReceived(LogEntry {
        timestamp: "2026-09-21 20:00:00".to_string(),
        priority: Priority::Info,
        message: "test log message".to_string(),
    }));
    let _ = log_view.update(LogMessage::LogReceived(LogEntry {
        timestamp: "2026-09-21 20:00:01".to_string(),
        priority: Priority::Error,
        message: "error message".to_string(),
    }));
    let _ = log_view.view();
}
