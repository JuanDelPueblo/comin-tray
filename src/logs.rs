use std::collections::HashMap;

use iced::futures::{SinkExt, channel::mpsc::Sender};
use serde::Deserialize;
use serde_json::Value;
use time::{OffsetDateTime, macros::format_description};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const TIMESTAMP_FORMAT: &[time::format_description::FormatItem] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");

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

    /// Maps the `level` field of a structured log line, as printed by
    /// Comin's own Go logger, to a journal priority.
    fn from_level(level: &str) -> Option<Self> {
        Some(match level.to_ascii_lowercase().as_str() {
            "trace" | "debug" => Priority::Debug,
            "info" => Priority::Info,
            "warn" | "warning" => Priority::Warning,
            "error" => Priority::Error,
            "fatal" | "panic" | "critical" => Priority::Critical,
            _ => return None,
        })
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
    let mut priority = raw
        .priority
        .as_ref()
        .map(value_as_string)
        .map(|code| Priority::from_code(&code))
        .unwrap_or(Priority::Info);
    let mut message = value_as_string(&raw.message);

    // Comin prints its own structured log lines to stdout, so journald only
    // ever sees them as plain, uniformly-"info" text. Pull the real level
    // and message back out of that line when it looks like `key=value` pairs.
    if let Some(fields) = parse_logfmt(&message) {
        if let Some(msg) = fields.get("msg") {
            message = msg.clone();
        }
        if let Some(level) = fields
            .get("level")
            .and_then(|level| Priority::from_level(level))
        {
            priority = level;
        }
    }

    Ok(LogEntry {
        timestamp,
        priority,
        message,
    })
}

/// Parses a `key=value key2="quoted value"` line, the format Comin's Go
/// logger uses. Returns `None` for anything that isn't logfmt-shaped, or
/// that doesn't carry a `msg` field, so unrelated plain-text journal lines
/// are left untouched.
fn parse_logfmt(input: &str) -> Option<HashMap<String, String>> {
    let mut fields = HashMap::new();
    let mut chars = input.chars().peekable();

    while chars.peek().is_some() {
        while chars.peek() == Some(&' ') {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ' ' {
                break;
            }
            key.push(c);
            chars.next();
        }

        if key.is_empty() || chars.peek() != Some(&'=') {
            return None;
        }
        chars.next(); // consume '='

        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next(); // consume the opening quote
            loop {
                match chars.next() {
                    Some('\\') => {
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    }
                    Some('"') | None => break,
                    Some(c) => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }

        fields.insert(key, value);
    }

    fields.contains_key("msg").then_some(fields)
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
    time.format(&TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| timestamp.into())
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

    #[test]
    fn structured_comin_lines_are_unwrapped() {
        let line = r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":"time=\"2026-09-19T19:08:25-04:00\" level=info msg=\"nix: switch successfully terminated\""}"#;
        let entry = parse_log_line(line).unwrap();

        assert_eq!(entry.priority, Priority::Info);
        assert_eq!(entry.message, "nix: switch successfully terminated");
    }

    #[test]
    fn structured_comin_lines_use_their_own_level() {
        let line = r#"{"__REALTIME_TIMESTAMP":"0","PRIORITY":"6","MESSAGE":"time=\"2026-09-19T19:08:25-04:00\" level=error msg=\"deploy failed\""}"#;
        let entry = parse_log_line(line).unwrap();

        assert_eq!(entry.priority, Priority::Error);
        assert_eq!(entry.message, "deploy failed");
    }

    #[test]
    fn plain_messages_are_left_untouched() {
        let line =
            r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":"restarting sysinit-reactivation.target"}"#;
        let entry = parse_log_line(line).unwrap();

        assert_eq!(entry.message, "restarting sysinit-reactivation.target");
    }
}
