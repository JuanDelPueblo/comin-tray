//! Turns the Comin journal records of one generation into an `nh`/`nom`-like
//! summary: stages with durations, the derivations nix built, the paths it
//! fetched, errors, and the raw lines grouped by stage.

use std::borrow::Cow;

use time::{Duration, OffsetDateTime};

use crate::{
    format::short_store_path,
    logs::{LogEntry, Priority},
};

/// How a journal line relates to the deployment it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// Bookkeeping or tool chatter that says nothing about the outcome.
    Noise,
    /// A lifecycle message from Comin (evaluating, building, deploying).
    Step,
    /// Comin runs an external command (`nix: running '…'`).
    Command,
    /// nix announces what it will build or fetch, or lists one such path.
    Plan,
    /// nix starts building a derivation.
    Building(String),
    /// nix downloads a path from a binary cache.
    Fetching {
        name: String,
        cache: String,
    },
    /// Output of a derivation's builder (`name> …`, from `nix build -L`).
    BuildOutput(String),
    /// Output of `switch-to-configuration` or the boot loader installer.
    Activation,
    Error,
    Warning,
    Other,
}

impl LineKind {
    pub fn is_noise(&self) -> bool {
        matches!(self, LineKind::Noise)
    }
}

const COMIN_NOISE_PREFIXES: &[&str] = &[
    "store: adding to the list",
    "store: removing from the list",
    "store: generation ",
    "nix: generating the comin.service unit file",
    "nix: the comin.service unit file sha256",
    "New commits have been fetched from",
    "server: start to stream events",
    "server: failed to send stream",
    "confirmer: ",
    "manager: the build of the generation",
];

const TOOL_NOISE_PREFIXES: &[&str] = &[
    "remote: ",
    "From file://",
    " * branch ",
    "fatal: Refusing to point HEAD outside of refs/",
    "warning: could not read HEAD ref from repo",
    "warning: could not update cached head",
    "Not checking switch inhibitors",
    "[binary data]",
];

pub fn classify(entry: &LogEntry) -> LineKind {
    let message = entry.message.as_str();

    if entry.from_comin {
        if entry.priority <= Priority::Error {
            return LineKind::Error;
        }
        if entry.priority == Priority::Warning {
            return LineKind::Warning;
        }
        if COMIN_NOISE_PREFIXES.iter().any(|p| message.starts_with(p))
            || (message.starts_with("deployer: out path ") && message.contains(" differs from "))
        {
            return LineKind::Noise;
        }
        if message.starts_with("nix: running ") {
            return LineKind::Command;
        }
        return LineKind::Step;
    }

    if entry.identifier == "bootctl" || entry.identifier.starts_with("switch-to-configuration") {
        return LineKind::Activation;
    }
    if TOOL_NOISE_PREFIXES.iter().any(|p| message.starts_with(p)) || is_blob_placeholder(message) {
        return LineKind::Noise;
    }
    if message.starts_with("error:") || message.starts_with("error (ignored):") {
        return LineKind::Error;
    }
    if let Some(drv) = quoted_after(message, "building '") {
        return LineKind::Building(derivation_name(drv).to_string());
    }
    if let Some(path) = quoted_after(message, "copying path '") {
        let cache = message
            .split_once("' from '")
            .and_then(|(_, rest)| rest.split('\'').next())
            .unwrap_or("")
            .to_string();
        return LineKind::Fetching {
            name: short_store_path(path).to_string(),
            cache,
        };
    }
    if is_plan_header(message) || message.starts_with("  /nix/store/") {
        return LineKind::Plan;
    }
    if let Some((name, rest)) = message.split_once("> ")
        && !name.is_empty()
        && !name.contains(' ')
    {
        return if rest.trim() == "structuredAttrs is enabled" {
            LineKind::Noise
        } else {
            LineKind::BuildOutput(name.to_string())
        };
    }
    if message.starts_with("warning:") {
        return LineKind::Warning;
    }
    if entry.priority <= Priority::Error {
        return LineKind::Error;
    }
    LineKind::Other
}

/// `[5K blob data]` is how `journalctl` shows binary fields in text output.
fn is_blob_placeholder(message: &str) -> bool {
    message.starts_with('[') && message.ends_with(" blob data]")
}

fn is_plan_header(message: &str) -> bool {
    (message.starts_with("these ") || message.starts_with("this "))
        && (message.contains(" will be built") || message.contains(" will be fetched"))
}

fn quoted_after<'a>(message: &'a str, prefix: &str) -> Option<&'a str> {
    message.strip_prefix(prefix)?.split('\'').next()
}

/// `/nix/store/<hash>-home-manager-files.drv` → `home-manager-files`.
pub fn derivation_name(path: &str) -> &str {
    let name = short_store_path(path);
    name.strip_suffix(".drv").unwrap_or(name)
}

/// Shortens the long commands Comin logs, so they fit on one row:
/// drops the always-present `--extra-experimental-features …` flags and
/// replaces the local repository URL with the short commit.
pub fn display_message(entry: &LogEntry) -> Cow<'_, str> {
    let message = entry.message.as_str();
    if !entry.from_comin || !message.contains("git+file://") && !message.contains("--extra-") {
        return Cow::Borrowed(message);
    }

    let mut shortened = message.replace(
        "nix --extra-experimental-features flakes nix-command --accept-flake-config ",
        "nix ",
    );
    while let Some(start) = shortened.find("git+file://") {
        let rest = &shortened[start..];
        let end = rest.find('#').unwrap_or(rest.len());
        let url = &rest[..end];
        let rev = url
            .split("rev=")
            .nth(1)
            .map(|rev| rev.split('&').next().unwrap_or(rev))
            .map(|rev| &rev[..rev.len().min(8)])
            .unwrap_or("repo");
        let replacement = format!("<repo@{rev}>");
        shortened.replace_range(start..start + end, &replacement);
    }
    Cow::Owned(shortened)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Evaluate,
    Build,
    Deploy,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::Evaluate, Stage::Build, Stage::Deploy];

    pub fn label(self) -> &'static str {
        match self {
            Stage::Evaluate => "Evaluate",
            Stage::Build => "Build",
            Stage::Deploy => "Deploy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    /// No record reached this stage.
    NotReached,
    Running,
    Done,
    Failed,
    /// Comin decided the stage was unnecessary (for example the output was
    /// already deployed).
    Skipped,
}

impl StageStatus {
    pub fn icon(self) -> &'static str {
        match self {
            StageStatus::NotReached => "○",
            StageStatus::Running => "◐",
            StageStatus::Done => "✓",
            StageStatus::Failed => "✗",
            StageStatus::Skipped => "⤼",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StageSummary {
    pub stage: Stage,
    pub status: StageStatus,
    pub start: Option<OffsetDateTime>,
    pub end: Option<OffsetDateTime>,
    /// Indices into [`BuildLog::entries`].
    pub lines: Vec<usize>,
}

impl StageSummary {
    pub fn duration(&self) -> Option<Duration> {
        Some(self.end? - self.start?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationState {
    Building,
    Built,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Derivation {
    pub name: String,
    pub started: Option<OffsetDateTime>,
    pub state: DerivationState,
    /// Lines of builder output seen for this derivation.
    pub output_lines: usize,
}

#[derive(Debug, Clone)]
pub struct Download {
    pub name: String,
    pub cache: String,
    pub started: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct BuildLog {
    pub entries: Vec<LogEntry>,
    pub kinds: Vec<LineKind>,
    pub stages: Vec<StageSummary>,
    /// Derivations nix announced it would build.
    pub planned_builds: usize,
    /// Paths nix announced it would fetch, and the size note it printed.
    pub planned_fetches: usize,
    pub fetch_size: Option<String>,
    pub derivations: Vec<Derivation>,
    pub downloads: Vec<Download>,
    /// Indices of error lines, shown above the summary.
    pub errors: Vec<usize>,
    pub out_path: Option<String>,
    /// Why Comin skipped the deployment, when it did.
    pub skipped: Option<String>,
    /// `true` while the generation is still being processed.
    pub live: bool,
}

impl BuildLog {
    /// Builds the summary from the records of one generation, in order.
    pub fn from_entries(entries: Vec<LogEntry>, live: bool) -> Self {
        let kinds: Vec<LineKind> = entries.iter().map(classify).collect();
        let mut log = Self {
            entries,
            kinds,
            stages: Stage::ALL
                .iter()
                .map(|&stage| StageSummary {
                    stage,
                    status: StageStatus::NotReached,
                    start: None,
                    end: None,
                    lines: Vec::new(),
                })
                .collect(),
            planned_builds: 0,
            planned_fetches: 0,
            fetch_size: None,
            derivations: Vec::new(),
            downloads: Vec::new(),
            errors: Vec::new(),
            out_path: None,
            skipped: None,
            live,
        };
        log.analyze();
        log
    }

    pub fn stage(&self, stage: Stage) -> &StageSummary {
        &self.stages[stage as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn analyze(&mut self) {
        let mut stage = Stage::Evaluate;
        let mut finished = [false; 3];

        for index in 0..self.entries.len() {
            let entry = &self.entries[index];
            let message = entry.message.as_str();
            let kind = &self.kinds[index];

            if entry.from_comin {
                if message.starts_with("builder: build of generation")
                    || (message.starts_with("nix: running ") && message.contains(" build "))
                {
                    stage = stage.max(Stage::Build);
                } else if message.starts_with("deployer: submitting generation")
                    || message.starts_with("deployer: deploying generation")
                {
                    stage = Stage::Deploy;
                }

                if message.starts_with("manager: a generation is available for deployment") {
                    finished[Stage::Evaluate as usize] = true;
                    // "Already built" generations never log a build step.
                    finished[Stage::Build as usize] = true;
                } else if message.starts_with("nix: command ")
                    && message.contains(" build ")
                    && message.ends_with("successfully executed")
                {
                    finished[Stage::Build as usize] = true;
                } else if message == "nix: deployment ended" {
                    finished[Stage::Deploy as usize] = true;
                } else if let Some(path) = message.strip_prefix("nix: the output path is ") {
                    self.out_path = Some(path.trim().to_string());
                } else if message.starts_with("deployer: skipping deployment") {
                    self.skipped = Some(
                        message
                            .split_once(": ")
                            .map_or(message, |(_, rest)| rest)
                            .split_once(": ")
                            .map_or(message, |(_, reason)| reason)
                            .to_string(),
                    );
                }
            }

            match kind {
                LineKind::Building(name) => {
                    stage = stage.max(Stage::Build);
                    self.derivations.push(Derivation {
                        name: name.clone(),
                        started: entry.time,
                        state: DerivationState::Building,
                        output_lines: 0,
                    });
                }
                LineKind::Fetching { name, cache } => self.downloads.push(Download {
                    name: name.clone(),
                    cache: cache.clone(),
                    started: entry.time,
                }),
                LineKind::BuildOutput(name) => {
                    // nix prefixes builder output with the derivation name,
                    // sometimes without its version suffix.
                    let exact = self
                        .derivations
                        .iter()
                        .rposition(|derivation| derivation.name == *name);
                    let found = exact.or_else(|| {
                        self.derivations
                            .iter()
                            .rposition(|derivation| derivation.name.starts_with(name.as_str()))
                    });
                    if let Some(position) = found {
                        self.derivations[position].output_lines += 1;
                    }
                }
                LineKind::Plan => {
                    if is_plan_header(message) {
                        let count = plan_count(message);
                        if message.contains(" will be built") {
                            self.planned_builds += count;
                        } else {
                            self.planned_fetches += count;
                            self.fetch_size = message
                                .split_once('(')
                                .and_then(|(_, rest)| rest.split_once(')'))
                                .map(|(size, _)| size.to_string());
                        }
                    }
                }
                LineKind::Error => {
                    self.errors.push(index);
                    if let Some(drv) = failed_derivation(message)
                        && let Some(derivation) = self
                            .derivations
                            .iter_mut()
                            .rev()
                            .find(|derivation| derivation.name == drv)
                    {
                        derivation.state = DerivationState::Failed;
                    }
                }
                _ => {}
            }

            let summary = &mut self.stages[stage as usize];
            summary.lines.push(index);
            if let Some(time) = entry.time {
                summary.start.get_or_insert(time);
                summary.end = Some(time);
            }
            if matches!(kind, LineKind::Error) {
                summary.status = StageStatus::Failed;
            }
        }

        let current = stage;
        for summary in &mut self.stages {
            if summary.status == StageStatus::Failed {
                continue;
            }
            let reached = !summary.lines.is_empty();
            summary.status =
                if finished[summary.stage as usize] || (reached && summary.stage < current) {
                    StageStatus::Done
                } else if reached && self.live {
                    StageStatus::Running
                } else if reached {
                    // The window ended without the closing marker (for example
                    // the journal was rotated); report what was seen as done.
                    StageStatus::Done
                } else {
                    StageStatus::NotReached
                };
        }
        if self.skipped.is_some() {
            self.stages[Stage::Deploy as usize].status = StageStatus::Skipped;
        }

        let build_failed = self.stage(Stage::Build).status == StageStatus::Failed;
        let build_running = self.stage(Stage::Build).status == StageStatus::Running;
        for derivation in &mut self.derivations {
            if derivation.state == DerivationState::Building && !build_running {
                derivation.state = if build_failed {
                    DerivationState::Failed
                } else {
                    DerivationState::Built
                };
            }
        }
        // While building, nix builds one derivation at a time per job; all
        // but the most recent ones have finished once a later one starts.
        if build_running {
            let jobs = 1;
            let running_from = self.derivations.len().saturating_sub(jobs);
            for derivation in &mut self.derivations[..running_from] {
                if derivation.state == DerivationState::Building {
                    derivation.state = DerivationState::Built;
                }
            }
        }
    }

    /// Indices of the lines to show, optionally hiding noise and limiting
    /// the list to one stage.
    pub fn visible_lines(&self, stage: Option<Stage>, hide_noise: bool) -> Vec<usize> {
        let keep = |index: &usize| !hide_noise || !self.kinds[*index].is_noise();
        match stage {
            Some(stage) => self
                .stage(stage)
                .lines
                .iter()
                .copied()
                .filter(keep)
                .collect(),
            None => (0..self.entries.len()).filter(keep).collect(),
        }
    }

    /// A one-line result for a stage, like `27 built · 13 fetched (…)`.
    pub fn stage_detail(&self, stage: Stage) -> String {
        match stage {
            Stage::Evaluate => {
                let fetched = self
                    .stage(Stage::Evaluate)
                    .lines
                    .iter()
                    .filter(|&&index| matches!(self.kinds[index], LineKind::Fetching { .. }))
                    .count();
                if fetched > 0 {
                    format!("{fetched} flake inputs fetched")
                } else {
                    String::new()
                }
            }
            Stage::Build => {
                let mut parts = Vec::new();
                let built = self.derivations.len();
                if self.planned_builds > 0 || built > 0 {
                    let total = self.planned_builds.max(built);
                    if self.stage(Stage::Build).status == StageStatus::Running {
                        parts.push(format!("building {built}/{total}"));
                    } else {
                        parts.push(format!("{total} built"));
                    }
                }
                let fetched = self
                    .stage(Stage::Build)
                    .lines
                    .iter()
                    .filter(|&&index| matches!(self.kinds[index], LineKind::Fetching { .. }))
                    .count();
                if self.planned_fetches > 0 || fetched > 0 {
                    let total = self.planned_fetches.max(fetched);
                    let mut part = format!("{total} fetched");
                    if let Some(size) = &self.fetch_size {
                        part.push_str(&format!(" ({size})"));
                    }
                    parts.push(part);
                }
                if parts.is_empty() && self.stage(Stage::Build).status == StageStatus::Done {
                    parts.push("nothing to build".into());
                }
                parts.join(" · ")
            }
            Stage::Deploy => {
                if let Some(reason) = &self.skipped {
                    return reason.clone();
                }
                self.stage(Stage::Deploy)
                    .lines
                    .iter()
                    .map(|&index| self.entries[index].message.as_str())
                    .find_map(|message| {
                        message
                            .strip_prefix("deployer: deploying generation ")
                            .and_then(|rest| rest.rsplit(' ').next())
                            .map(|operation| format!("operation {operation}"))
                    })
                    .unwrap_or_default()
            }
        }
    }
}

fn plan_count(header: &str) -> usize {
    if header.starts_with("this ") {
        return 1;
    }
    header
        .split_whitespace()
        .nth(1)
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

/// `error: builder for '/nix/store/<hash>-foo.drv' failed …` → `foo`.
fn failed_derivation(message: &str) -> Option<&str> {
    let rest = message
        .strip_prefix("error: builder for '")
        .or_else(|| message.strip_prefix("error: Cannot build '"))?;
    rest.split('\'').next().map(derivation_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::parse_log_line;

    fn fixture() -> Vec<LogEntry> {
        include_str!("../tests/fixtures/journal_boot_deploy.jsonl")
            .lines()
            .map(|line| parse_log_line(line).unwrap().0)
            .collect()
    }

    /// A wall-clock time of the fixture, which was recorded at UTC-4.
    fn at(local: &str) -> OffsetDateTime {
        let parts: Vec<i64> = local.split(':').map(|part| part.parse().unwrap()).collect();
        time::macros::datetime!(2026-09-23 0:00 -4)
            + Duration::hours(parts[0])
            + Duration::minutes(parts[1])
            + Duration::seconds(parts[2])
    }

    fn window(entries: &[LogEntry], from: &str, to: &str) -> Vec<LogEntry> {
        let from = at(from);
        let to = at(to) + Duration::seconds(1);
        entries
            .iter()
            .filter(|entry| entry.time.is_some_and(|time| time >= from && time < to))
            .cloned()
            .collect()
    }

    #[test]
    fn boot_deployment_is_split_into_stages() {
        let entries = fixture();
        let log = BuildLog::from_entries(window(&entries, "19:39:56", "19:41:51"), false);

        assert_eq!(log.stage(Stage::Evaluate).status, StageStatus::Done);
        assert_eq!(log.stage(Stage::Build).status, StageStatus::Done);
        assert_eq!(log.stage(Stage::Deploy).status, StageStatus::Done);

        let seconds = |stage| log.stage(stage).duration().map(|d| d.whole_seconds());
        assert_eq!(seconds(Stage::Evaluate), Some(51));
        assert_eq!(seconds(Stage::Build), Some(63));

        assert_eq!(log.planned_builds, 27);
        assert_eq!(log.planned_fetches, 13);
        assert_eq!(
            log.fetch_size.as_deref(),
            Some("0.0 KiB download, 3.2 GiB unpacked")
        );
        assert_eq!(log.derivations.len(), 27);
        assert!(
            log.derivations
                .iter()
                .all(|derivation| derivation.state == DerivationState::Built)
        );
        assert_eq!(log.derivations[0].name, "options.json");
        let system_path = log
            .derivations
            .iter()
            .find(|derivation| derivation.name == "system-path")
            .unwrap();
        assert_eq!(system_path.output_lines, 2);
        assert_eq!(
            log.stage_detail(Stage::Build),
            "27 built · 13 fetched (0.0 KiB download, 3.2 GiB unpacked)"
        );
        assert_eq!(log.stage_detail(Stage::Evaluate), "2 flake inputs fetched");
        assert_eq!(log.stage_detail(Stage::Deploy), "operation boot");
        assert!(log.errors.is_empty());
        assert_eq!(
            log.out_path.as_deref(),
            Some(
                "/nix/store/lp2prv1h97kqp7d7i5pgzjk4gqm3n0yv-nixos-system-Ed-PCL-26.11.20260922.6774f7b"
            )
        );
    }

    #[test]
    fn activation_lines_belong_to_the_deploy_stage() {
        let entries = fixture();
        let log = BuildLog::from_entries(window(&entries, "19:39:56", "19:41:51"), false);
        let deploy = log.stage(Stage::Deploy);
        let bootctl = deploy
            .lines
            .iter()
            .filter(|&&index| log.entries[index].identifier == "bootctl")
            .count();
        assert_eq!(bootctl, 3);
        assert!(
            deploy
                .lines
                .iter()
                .all(|&index| log.kinds[index] != LineKind::Plan)
        );
    }

    #[test]
    fn noise_is_classified_and_hidden() {
        let entries = fixture();
        let log = BuildLog::from_entries(window(&entries, "19:39:56", "19:41:51"), false);
        let noise: Vec<&str> = log
            .entries
            .iter()
            .zip(&log.kinds)
            .filter(|(_, kind)| kind.is_noise())
            .map(|(entry, _)| entry.message.as_str())
            .collect();

        for expected in [
            "[binary data]",
            "remote: Enumerating objects: 4491, done.",
            "fatal: Refusing to point HEAD outside of refs/",
            "home-manager-auto-expire> structuredAttrs is enabled",
            "confirmer: confirmed generation d558cf65-2df9-4869-8855-74165854ddf9",
            "nix: the comin.service unit file sha256 is 'f9a192889da6ea7e98aaaa58b0c04f31c23af8d37a160baf074e6be6c1429429'",
        ] {
            assert!(noise.contains(&expected), "{expected} should be noise");
        }
        assert!(!noise.iter().any(|line| line.starts_with("building '")));
        assert!(
            !noise
                .iter()
                .any(|line| line.starts_with("home-manager> install"))
        );

        let visible = log.visible_lines(None, true);
        assert!(visible.len() < log.entries.len());
        assert_eq!(log.visible_lines(None, false).len(), log.entries.len());
    }

    #[test]
    fn already_deployed_generation_is_skipped() {
        let entries = fixture();
        let log = BuildLog::from_entries(window(&entries, "16:15:56", "16:16:09"), false);

        assert_eq!(log.stage(Stage::Evaluate).status, StageStatus::Done);
        assert!(log.stage(Stage::Build).lines.is_empty());
        assert_eq!(log.stage(Stage::Deploy).status, StageStatus::Skipped);
        assert!(log.derivations.is_empty());
        assert!(
            log.stage_detail(Stage::Deploy)
                .ends_with("with operation boot has already been deployed")
        );
    }

    #[test]
    fn live_build_reports_progress() {
        let entries = fixture();
        let log = BuildLog::from_entries(window(&entries, "19:39:56", "19:40:55"), true);

        assert_eq!(log.stage(Stage::Evaluate).status, StageStatus::Done);
        assert_eq!(log.stage(Stage::Build).status, StageStatus::Running);
        assert_eq!(log.stage(Stage::Deploy).status, StageStatus::NotReached);
        assert_eq!(log.derivations.len(), 10);
        assert!(log.stage_detail(Stage::Build).starts_with("building 10/27"));
        assert_eq!(
            log.derivations.last().unwrap().state,
            DerivationState::Building
        );
    }

    #[test]
    fn failed_builds_surface_the_error() {
        let mut entries = window(&fixture(), "19:39:56", "19:40:53");
        let mut failure = entries.last().unwrap().clone();
        failure.message = "error: builder for '/nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm211p2-home-manager.drv' failed with exit code 1".into();
        entries.push(failure);
        let log = BuildLog::from_entries(entries, false);

        assert_eq!(log.errors.len(), 1);
        assert_eq!(log.stage(Stage::Build).status, StageStatus::Failed);
        let home_manager = log
            .derivations
            .iter()
            .find(|derivation| derivation.name == "home-manager")
            .unwrap();
        assert_eq!(home_manager.state, DerivationState::Failed);
    }

    #[test]
    fn long_nix_commands_are_shortened_for_display() {
        let entries = fixture();
        let command = entries
            .iter()
            .find(|entry| entry.message.starts_with("nix: running 'nix --extra"))
            .unwrap();
        assert_eq!(
            display_message(command),
            "nix: running 'nix derivation show <repo@5c252993>#nixosConfigurations.\"Ed-PCL\".config.system.build.toplevel -L --show-trace'"
        );
    }

    #[test]
    fn derivation_names_drop_hash_and_suffix() {
        assert_eq!(
            derivation_name("/nix/store/wh7zlc08m5lpvs514pmydrda3wdq4cin-home-manager-files.drv"),
            "home-manager-files"
        );
    }
}
