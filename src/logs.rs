use std::{collections::HashMap, time::Duration};

use iced::futures::{SinkExt, channel::mpsc::Sender};
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

use crate::format::{format_clock, format_local_datetime};

/// Journal records loaded when the live stream starts.
const INITIAL_LINES: usize = 500;

/// Longest wait between attempts to restart a `journalctl --follow` that died.
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// When journald received the record. `None` for notes the GUI adds.
    pub time: Option<OffsetDateTime>,
    pub priority: Priority,
    /// `SYSLOG_IDENTIFIER`, such as `comin` or `bootctl`.
    pub identifier: String,
    pub pid: Option<u32>,
    /// The message, without ANSI escapes and with Comin's logfmt unwrapped.
    pub message: String,
    /// `true` when the line came from Comin's own logger (`level=… msg=…`),
    /// `false` for output of the commands Comin runs (nix, git, activation).
    pub from_comin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    /// A fixed-width tag for copied text, so pasted columns line up.
    pub fn short_label(self) -> &'static str {
        match self {
            Priority::Emergency => "EMERG",
            Priority::Alert => "ALERT",
            Priority::Critical => "CRIT ",
            Priority::Error => "ERROR",
            Priority::Warning => "WARN ",
            Priority::Notice => "NOTE ",
            Priority::Info => "INFO ",
            Priority::Debug => "DEBUG",
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
    /// A message from comin-tray itself, such as a journal read error.
    pub fn note(priority: Priority, message: impl Into<String>) -> Self {
        Self {
            time: None,
            priority,
            identifier: "comin-tray".into(),
            pid: None,
            message: message.into(),
            from_comin: false,
        }
    }

    /// `HH:MM:SS` in local time, or an empty string for notes.
    pub fn clock(&self) -> String {
        self.time.map(format_clock).unwrap_or_default()
    }

    /// The line as it is copied to the clipboard or saved to a file.
    pub fn to_copy_line(&self) -> String {
        let time = self.time.map(format_local_datetime).unwrap_or_default();
        let source = if self.identifier.is_empty() || self.identifier == "comin" {
            String::new()
        } else {
            format!("[{}] ", self.identifier)
        };
        format!(
            "{time} {} {source}{}",
            self.priority.short_label(),
            self.message
        )
        .trim_start()
        .to_string()
    }
}

/// Joins entries into clipboard text, one line per entry.
pub fn to_copy_text<'a>(entries: impl IntoIterator<Item = &'a LogEntry>) -> String {
    let mut text = String::new();
    for entry in entries {
        text.push_str(&entry.to_copy_line());
        text.push('\n');
    }
    text
}

/// The `journalctl` executable. `COMIN_TRAY_JOURNALCTL` overrides it, which
/// lets the GUI run against recorded fixtures.
pub fn journalctl() -> Command {
    let program = std::env::var("COMIN_TRAY_JOURNALCTL").unwrap_or_else(|_| "journalctl".into());
    let mut command = Command::new(program);
    command.kill_on_drop(true);
    command
}

const JOURNAL_ARGS: [&str; 4] = [
    "--unit=comin.service",
    "--output=json",
    "--all",
    "--no-pager",
];

/// Streams the last `INITIAL_LINES` records of the Comin service, then
/// follows new ones. When `journalctl` exits, it is restarted after the last
/// record it delivered, so the stream survives journald restarts.
pub async fn stream(sender: &mut Sender<LogEntry>) {
    let mut cursor: Option<String> = None;
    let mut delay = Duration::from_secs(1);

    loop {
        let mut command = journalctl();
        command.args(JOURNAL_ARGS).arg("--follow");
        match &cursor {
            Some(cursor) => command.arg(format!("--after-cursor={cursor}")),
            None => command.arg(format!("--lines={INITIAL_LINES}")),
        };
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        match command.spawn() {
            Ok(mut journal) => {
                let delivered = follow(&mut journal, sender, &mut cursor).await;
                let _ = journal.kill().await;
                if sender.is_closed() {
                    return;
                }
                if delivered {
                    delay = Duration::from_secs(1);
                }
                let _ = sender
                    .send(LogEntry::note(
                        Priority::Warning,
                        format!(
                            "The journal stream stopped; reconnecting in {}s",
                            delay.as_secs()
                        ),
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

        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(MAX_RECONNECT_DELAY);
    }
}

/// Forwards records until `journalctl` exits. Returns whether any record was
/// delivered.
async fn follow(
    journal: &mut tokio::process::Child,
    sender: &mut Sender<LogEntry>,
    cursor: &mut Option<String>,
) -> bool {
    let Some(stdout) = journal.stdout.take() else {
        return false;
    };
    let stderr = journal.stderr.take();
    let mut lines = BufReader::new(stdout).lines();
    let mut delivered = false;

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let (entry, next_cursor) = parse_line(&line);
                if next_cursor.is_some() {
                    *cursor = next_cursor;
                }
                delivered = true;
                if sender.send(entry).await.is_err() {
                    return delivered;
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

    // Surface journalctl's own complaint, such as a missing permission.
    if let Some(stderr) = stderr {
        let mut text = String::new();
        let mut stderr = BufReader::new(stderr);
        let _ = tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut text).await;
        let text = text.trim();
        if !text.is_empty() {
            let _ = sender
                .send(LogEntry::note(
                    Priority::Warning,
                    format!("journalctl: {text}"),
                ))
                .await;
        }
    }

    delivered
}

/// Reads the Comin journal records between two instants.
pub async fn read_range(
    since: OffsetDateTime,
    until: Option<OffsetDateTime>,
) -> Result<Vec<LogEntry>, String> {
    let mut command = journalctl();
    command
        .args(JOURNAL_ARGS)
        .arg(format!("--since=@{}", since.unix_timestamp()))
        .stdin(std::process::Stdio::null());
    if let Some(until) = until {
        command.arg(format!("--until=@{}", until.unix_timestamp()));
    }

    let output = command
        .output()
        .await
        .map_err(|error| format!("Could not start journalctl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "journalctl failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| parse_line(line).0)
        .collect())
}

/// Checks that the current user can read the Comin journal.
pub async fn check_journal_access() -> Result<(), String> {
    let output = journalctl()
        .args([
            "--unit=comin.service",
            "--output=json",
            "--lines=1",
            "--no-pager",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| format!("Could not start journalctl: {error}"))?;

    if output.status.success() && !output.stdout.trim_ascii().is_empty() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let hint = stderr.trim();
    Err(if hint.is_empty() {
        "no comin.service records are visible; add your user to the `systemd-journal` group".into()
    } else {
        format!("no comin.service records are visible: {hint}")
    })
}

fn parse_line(line: &str) -> (LogEntry, Option<String>) {
    match parse_log_line(line) {
        Ok(parsed) => parsed,
        Err(error) => (
            LogEntry::note(Priority::Error, format!("Journal parse error: {error}")),
            None,
        ),
    }
}

#[derive(Deserialize)]
struct RawLogEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    timestamp: Value,
    #[serde(rename = "__CURSOR")]
    cursor: Option<String>,
    #[serde(rename = "PRIORITY")]
    priority: Option<Value>,
    #[serde(rename = "SYSLOG_IDENTIFIER")]
    identifier: Option<Value>,
    #[serde(rename = "_PID")]
    pid: Option<Value>,
    #[serde(rename = "MESSAGE")]
    message: Value,
}

pub fn parse_log_line(line: &str) -> Result<(LogEntry, Option<String>), String> {
    let raw: RawLogEntry = serde_json::from_str(line)
        .map_err(|error| format!("journalctl returned invalid JSON: {error}"))?;
    let time = value_as_string(&raw.timestamp)
        .parse::<i128>()
        .ok()
        .and_then(|micros| OffsetDateTime::from_unix_timestamp_nanos(micros * 1_000).ok());
    let mut priority = raw
        .priority
        .as_ref()
        .map(value_as_string)
        .map(|code| Priority::from_code(&code))
        .unwrap_or(Priority::Info);
    let mut message = clean_message(&value_as_string(&raw.message));
    let mut from_comin = false;

    // Comin prints its own structured log lines to stdout, so journald only
    // ever sees them as plain, uniformly-"info" text. Pull the real level
    // and message back out of that line when it looks like `key=value` pairs.
    if let Some(fields) = parse_logfmt(&message) {
        from_comin = true;
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

    let entry = LogEntry {
        time,
        priority,
        identifier: raw
            .identifier
            .as_ref()
            .map(value_as_string)
            .unwrap_or_default(),
        pid: raw
            .pid
            .as_ref()
            .and_then(|pid| value_as_string(pid).parse().ok()),
        message,
        from_comin,
    };
    Ok((entry, raw.cursor))
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

/// journald's JSON output stores a field as a string, as an array of bytes
/// when it is not printable UTF-8 (colored nix output, git progress), or as
/// `null` when it is too large to print.
fn value_as_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(items) => {
            let bytes: Vec<u8> = items
                .iter()
                .filter_map(|item| item.as_u64().and_then(|byte| u8::try_from(byte).ok()))
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
        Value::Null => "[binary data]".into(),
        value => value.to_string(),
    }
}

/// Removes ANSI escape sequences and keeps only the final state of lines
/// redrawn with carriage returns (git and nix progress output).
fn clean_message(message: &str) -> String {
    let mut clean = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                // CSI: ESC [ params final-byte; other escapes: ESC + one char.
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                } else {
                    chars.next();
                }
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                // A bare carriage return redraws the current line.
                let line_start = clean.rfind('\n').map_or(0, |index| index + 1);
                if chars.peek().is_some() {
                    clean.truncate(line_start);
                }
            }
            c => clean.push(c),
        }
    }
    clean.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> LogEntry {
        parse_log_line(line).unwrap().0
    }

    #[test]
    fn json_log_entries_have_clear_fields() {
        let (entry, cursor) = parse_log_line(
            r#"{"__REALTIME_TIMESTAMP":"0","__CURSOR":"s=1","PRIORITY":"3","SYSLOG_IDENTIFIER":"comin","_PID":"947","MESSAGE":"deploy failed"}"#,
        )
        .unwrap();

        assert_eq!(entry.time, Some(OffsetDateTime::UNIX_EPOCH));
        assert_eq!(entry.priority, Priority::Error);
        assert_eq!(entry.identifier, "comin");
        assert_eq!(entry.pid, Some(947));
        assert_eq!(entry.message, "deploy failed");
        assert!(!entry.from_comin);
        assert_eq!(cursor.as_deref(), Some("s=1"));
    }

    #[test]
    fn structured_comin_lines_are_unwrapped() {
        let line = r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":"time=\"2026-09-19T19:08:25-04:00\" level=info msg=\"nix: switch successfully terminated\""}"#;
        let entry = parse(line);

        assert_eq!(entry.priority, Priority::Info);
        assert_eq!(entry.message, "nix: switch successfully terminated");
        assert!(entry.from_comin);
    }

    #[test]
    fn structured_comin_lines_use_their_own_level() {
        let line = r#"{"__REALTIME_TIMESTAMP":"0","PRIORITY":"6","MESSAGE":"time=\"2026-09-19T19:08:25-04:00\" level=error msg=\"deploy failed\""}"#;
        let entry = parse(line);

        assert_eq!(entry.priority, Priority::Error);
        assert_eq!(entry.message, "deploy failed");
    }

    #[test]
    fn plain_messages_are_left_untouched() {
        let line =
            r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":"restarting sysinit-reactivation.target"}"#;
        let entry = parse(line);

        assert_eq!(entry.message, "restarting sysinit-reactivation.target");
    }

    #[test]
    fn byte_array_messages_are_decoded_and_stripped_of_colors() {
        // "\x1b[31;1merror:\x1b[0m boom"
        let line = r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":[27,91,51,49,59,49,109,101,114,114,111,114,58,27,91,48,109,32,98,111,111,109]}"#;
        assert_eq!(parse(line).message, "error: boom");
    }

    #[test]
    fn carriage_return_progress_keeps_the_last_state() {
        assert_eq!(
            clean_message("Receiving objects: 10%\rReceiving objects: 100%, done.\r\n"),
            "Receiving objects: 100%, done."
        );
        assert_eq!(clean_message("a\nb 1%\rb 2%"), "a\nb 2%");
    }

    #[test]
    fn null_messages_do_not_break_parsing() {
        let line = r#"{"__REALTIME_TIMESTAMP":"0","MESSAGE":null}"#;
        assert_eq!(parse(line).message, "[binary data]");
    }

    #[test]
    fn copy_lines_carry_time_level_and_source() {
        let mut entry = LogEntry::note(Priority::Warning, "hello");
        assert_eq!(entry.to_copy_line(), "WARN  [comin-tray] hello");
        entry.identifier = "comin".into();
        entry.time = Some(OffsetDateTime::UNIX_EPOCH);
        // Tests run multi-threaded, so the local offset falls back to UTC.
        assert_eq!(entry.to_copy_line(), "1970-01-01 00:00:00 WARN  hello");
    }
}
