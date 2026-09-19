use iced::futures::{SinkExt, channel::mpsc::Sender};
use serde::Deserialize;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const INITIAL_LINES: usize = 250;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub priority: Priority,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

impl Priority {
    pub fn label(self) -> &'static str {
        match self {
            Priority::Emergency => "Emergency",
            Priority::Alert => "Alert",
            Priority::Critical => "Critical",
            Priority::Error => "Error",
            Priority::Warning => "Warning",
            Priority::Notice => "Notice",
            Priority::Info => "Info",
            Priority::Debug => "Debug",
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "0" => Priority::Emergency,
            "1" => Priority::Alert,
            "2" => Priority::Critical,
            "3" => Priority::Error,
            "4" => Priority::Warning,
            "5" => Priority::Notice,
            "6" => Priority::Info,
            "7" => Priority::Debug,
            _ => Priority::Info,
        }
    }
}

impl LogEntry {
    fn note(priority: Priority, message: impl Into<String>) -> Self {
        Self {
            timestamp: String::new(),
            priority,
            message: message.into(),
        }
    }
}

/// Sends the last `INITIAL_LINES` journal records to `sender`.
pub async fn send_history(sender: &mut Sender<LogEntry>) {
    let output = Command::new("journalctl")
        .args(["--unit=comin.service", "--output=json", "--no-pager"])
        .arg(format!("--lines={INITIAL_LINES}"))
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let _ = sender.send(parse_line(line)).await;
            }
        }
        Ok(output) => {
            let message = String::from_utf8_lossy(&output.stderr);
            let _ = sender
                .send(LogEntry::note(
                    Priority::Error,
                    format!("Could not read logs: {message}"),
                ))
                .await;
        }
        Err(error) => {
            let _ = sender
                .send(LogEntry::note(
                    Priority::Error,
                    format!("Could not start journalctl: {error}"),
                ))
                .await;
        }
    }
}

/// Follows new journal records for the Comin service and sends them to `sender`.
pub async fn send_follow(sender: &mut Sender<LogEntry>) {
    let journal = Command::new("journalctl")
        .args([
            "--unit=comin.service",
            "--output=json",
            "--lines=0",
            "--follow",
            "--no-pager",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn();

    let mut journal = match journal {
        Ok(journal) => journal,
        Err(error) => {
            let _ = sender
                .send(LogEntry::note(
                    Priority::Error,
                    format!("Could not start the Comin log stream: {error}"),
                ))
                .await;
            return;
        }
    };

    let Some(stdout) = journal.stdout.take() else {
        return;
    };
    let mut lines = BufReader::new(stdout).lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if sender.send(parse_line(&line)).await.is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = sender
                    .send(LogEntry::note(
                        Priority::Error,
                        format!("Could not read the log stream: {error}"),
                    ))
                    .await;
                break;
            }
        }
    }

    let _ = journal.kill().await;
}

fn parse_line(line: &str) -> LogEntry {
    parse_log_line(line).unwrap_or_else(|error| {
        LogEntry::note(Priority::Error, format!("Journal parse error: {error}"))
    })
}

#[derive(Deserialize)]
struct RawLogEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    timestamp: Value,
    #[serde(rename = "PRIORITY")]
    priority: Option<Value>,
    #[serde(rename = "MESSAGE")]
    message: Value,
}

fn parse_log_line(line: &str) -> Result<LogEntry, String> {
    let raw: RawLogEntry = serde_json::from_str(line)
        .map_err(|error| format!("journalctl returned invalid JSON: {error}"))?;
    let timestamp = format_timestamp(&value_as_string(&raw.timestamp));
    let priority = raw
        .priority
        .as_ref()
        .map(value_as_string)
        .map(|code| Priority::from_code(&code))
        .unwrap_or(Priority::Info);

    Ok(LogEntry {
        timestamp,
        priority,
        message: value_as_string(&raw.message),
    })
}

fn value_as_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

fn format_timestamp(timestamp: &str) -> String {
    let Ok(microseconds) = timestamp.parse::<i128>() else {
        return timestamp.into();
    };
    let Ok(time) = OffsetDateTime::from_unix_timestamp_nanos(microseconds * 1_000) else {
        return timestamp.into();
    };
    time.format(&Rfc3339).unwrap_or_else(|_| timestamp.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_log_entries_have_clear_fields() {
        let entry = parse_log_line(
            r#"{"__REALTIME_TIMESTAMP":"0","PRIORITY":"3","MESSAGE":"deploy failed"}"#,
        )
        .unwrap();

        assert_eq!(entry.timestamp, "1970-01-01T00:00:00Z");
        assert_eq!(entry.priority, Priority::Error);
        assert_eq!(entry.message, "deploy failed");
    }
}
