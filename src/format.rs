use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn short_commit(commit: &str) -> &str {
    if commit.len() > 8 {
        &commit[..8]
    } else {
        commit
    }
}

pub fn commit_title(msg: &str) -> &str {
    msg.lines().next().unwrap_or("").trim()
}

pub fn short_store_path(path: &str) -> &str {
    path.strip_prefix("/nix/store/")
        .and_then(|rest| rest.split_once('-'))
        .map(|(_hash, name)| name)
        .unwrap_or(path)
}

pub fn format_relative_time(datetime_str: &str, now: OffsetDateTime) -> Option<String> {
    let dt = OffsetDateTime::parse(datetime_str, &Rfc3339).ok()?;
    let duration = now - dt;
    let seconds = duration.whole_seconds();

    if seconds < 0 {
        return Some("just now".to_string());
    }

    if seconds < 60 {
        return Some(format!("{seconds}s ago"));
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return Some(format!("{minutes}m ago"));
    }

    let hours = minutes / 60;
    if hours < 24 {
        return Some(format!("{hours}h ago"));
    }

    let days = hours / 24;
    Some(format!("{days}d ago"))
}

pub fn format_relative_time_now(datetime_str: &str) -> String {
    format_relative_time(datetime_str, OffsetDateTime::now_utc())
        .unwrap_or_else(|| datetime_str.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_commit() {
        assert_eq!(short_commit("1234567890abcdef"), "12345678");
        assert_eq!(short_commit("12345"), "12345");
        assert_eq!(short_commit(""), "");
    }

    #[test]
    fn test_commit_title() {
        assert_eq!(
            commit_title("feat: add something\n\nDetailed message"),
            "feat: add something"
        );
        assert_eq!(
            commit_title("  fix: some bug\r\nSecond line"),
            "fix: some bug"
        );
        assert_eq!(commit_title(""), "");
    }

    #[test]
    fn test_short_store_path() {
        assert_eq!(
            short_store_path("/nix/store/10hwhf8v0z8p222cfcf8867a5lcb7q01-nixos-system-Ed-PCL"),
            "nixos-system-Ed-PCL"
        );
        assert_eq!(short_store_path("/some/other/path"), "/some/other/path");
    }

    #[test]
    fn test_format_relative_time() {
        let base = OffsetDateTime::parse("2026-09-21T22:30:00Z", &Rfc3339).unwrap();

        // 10 seconds ago
        let t1 = "2026-09-21T22:29:50Z";
        assert_eq!(format_relative_time(t1, base), Some("10s ago".to_string()));

        // 5 minutes ago
        let t2 = "2026-09-21T22:25:00Z";
        assert_eq!(format_relative_time(t2, base), Some("5m ago".to_string()));

        // 3 hours ago
        let t3 = "2026-09-21T19:30:00Z";
        assert_eq!(format_relative_time(t3, base), Some("3h ago".to_string()));

        // 2 days ago
        let t4 = "2026-09-19T22:30:00Z";
        assert_eq!(format_relative_time(t4, base), Some("2d ago".to_string()));

        // invalid
        assert_eq!(format_relative_time("invalid", base), None);
    }
}
