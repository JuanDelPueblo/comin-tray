use serde::Deserialize;

use time::{Duration, OffsetDateTime};

use crate::format::{commit_title, format_relative_time_now, parse_rfc3339, short_commit};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    Fetching,
    Evaluating,
    Building,
    Deploying,
    Failed,
    Suspended,
    RebootRequired,
    Unavailable,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Fetching => "Fetching",
            Self::Evaluating => "Evaluating",
            Self::Building => "Building",
            Self::Deploying => "Deploying",
            Self::Failed => "Failed",
            Self::Suspended => "Suspended",
            Self::RebootRequired => "Reboot required",
            Self::Unavailable => "Unavailable",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Idle => "emblem-default",
            Self::Fetching => "view-refresh",
            Self::Evaluating => "system-run",
            Self::Building => "run-build",
            Self::Deploying => "system-software-update",
            Self::Failed => "dialog-error",
            Self::Suspended => "media-playback-pause",
            Self::RebootRequired => "system-reboot",
            Self::Unavailable => "network-error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TrayState {
    pub phase: Phase,
    pub status: Option<CominState>,
    pub error: Option<String>,
}

impl TrayState {
    pub fn from_status(status: CominState) -> Self {
        Self {
            phase: status.phase(),
            status: Some(status),
            error: None,
        }
    }

    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            phase: Phase::Unavailable,
            status: None,
            error: Some(error.into()),
        }
    }

    pub fn tooltip(&self) -> String {
        let Some(status) = &self.status else {
            return self
                .error
                .as_deref()
                .map(|_| "Open Comin for details".to_string())
                .unwrap_or_else(|| "Comin is unavailable".to_string());
        };

        let mut lines = Vec::new();

        let source_summary = status.source_summary();
        let hostname = status.hostname();
        let line1 = match (hostname, source_summary) {
            (Some(host), Some(source)) => format!("{host} · {source}"),
            (Some(host), None) => host.to_string(),
            (None, Some(source)) => source,
            (None, None) => "Unknown source".to_string(),
        };
        lines.push(line1);

        match self.phase {
            Phase::Evaluating => {
                if let Some(detail) = status.eval_started_relative() {
                    lines.push(format!("Evaluation started {detail}"));
                }
            }
            Phase::Building => {
                if let Some(detail) = status.build_started_relative() {
                    lines.push(format!("Build started {detail}"));
                }
            }
            Phase::Deploying => {
                if let Some(detail) = status.deploy_started_relative() {
                    lines.push(format!("Deployment started {detail}"));
                }
            }
            Phase::Failed => {
                lines.push("Open Comin for details".to_string());
            }
            _ => {}
        }

        lines.join("\n")
    }

    pub fn evaluation_message(&self) -> String {
        let generation = self
            .status
            .as_ref()
            .and_then(|status| status.builder.generation.as_ref());

        let Some(generation) = generation else {
            return "A new commit was fetched. Evaluation started.".into();
        };

        let commit = generation
            .selected_commit_id
            .as_deref()
            .map(short_commit)
            .unwrap_or_default();
        let remote = generation
            .selected_remote_name
            .as_deref()
            .unwrap_or("unknown");
        let branch = generation
            .selected_branch_name
            .as_deref()
            .unwrap_or("unknown");
        let source = format!("{remote}/{branch}");

        let title = generation
            .selected_commit_msg
            .as_deref()
            .map(commit_title)
            .unwrap_or_default();

        if !title.is_empty() && !commit.is_empty() {
            format!("Commit {commit} (\"{title}\") from {source} was fetched. Evaluation started.")
        } else if !commit.is_empty() {
            format!("Commit {commit} from {source} was fetched. Evaluation started.")
        } else {
            format!("A new commit from {source} was fetched. Evaluation started.")
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CominState {
    pub need_to_reboot: Option<bool>,
    pub is_suspended: Option<bool>,
    pub builder: Builder,
    pub deployer: Deployer,
    pub fetcher: Fetcher,
    pub store: Store,
    pub build_confirmer: Option<Confirmer>,
    pub deploy_confirmer: Option<Confirmer>,
}

impl CominState {
    pub fn hostname(&self) -> Option<&str> {
        self.builder
            .hostname
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.builder
                    .generation
                    .as_ref()
                    .and_then(|g| g.hostname.as_deref().filter(|s| !s.is_empty()))
            })
    }

    pub fn phase(&self) -> Phase {
        if self.is_suspended.unwrap_or(false) {
            Phase::Suspended
        } else if self.fetcher.is_fetching.unwrap_or(false) {
            Phase::Fetching
        } else if self.builder.is_evaluating.unwrap_or(false) {
            Phase::Evaluating
        } else if self.builder.is_building.unwrap_or(false) {
            Phase::Building
        } else if self.deployer.is_deploying.unwrap_or(false) {
            Phase::Deploying
        } else if self.failed() {
            Phase::Failed
        } else if self.need_to_reboot.unwrap_or(false) {
            Phase::RebootRequired
        } else {
            Phase::Idle
        }
    }

    pub fn source_summary(&self) -> Option<String> {
        if let Some(repo) = &self.fetcher.repository_status {
            let r_remote = repo
                .selected_remote_name
                .as_deref()
                .filter(|s| !s.is_empty());
            let r_branch = repo
                .selected_branch_name
                .as_deref()
                .filter(|s| !s.is_empty());
            let r_commit = repo.selected_commit_id.as_deref().filter(|s| !s.is_empty());
            if let (Some(remote), Some(branch), Some(commit)) = (r_remote, r_branch, r_commit) {
                return Some(format!("{remote}/{branch} @ {}", short_commit(commit)));
            }
        }

        let gen_opt = self
            .builder
            .generation
            .as_ref()
            .or_else(|| self.latest_deployment().and_then(|d| d.generation.as_ref()));

        if let Some(generation) = gen_opt {
            let g_remote = generation
                .selected_remote_name
                .as_deref()
                .filter(|s| !s.is_empty());
            let g_branch = generation
                .selected_branch_name
                .as_deref()
                .filter(|s| !s.is_empty());
            let g_commit = generation
                .selected_commit_id
                .as_deref()
                .filter(|s| !s.is_empty());
            if let (Some(remote), Some(branch), Some(commit)) = (g_remote, g_branch, g_commit) {
                return Some(format!("{remote}/{branch} @ {}", short_commit(commit)));
            }
        }

        None
    }

    pub fn latest_deployment(&self) -> Option<&Deployment> {
        self.store
            .deployments
            .iter()
            .max_by_key(|deployment| {
                deployment
                    .ended_at
                    .as_deref()
                    .or(deployment.created_at.as_deref())
                    .or(deployment.started_at.as_deref())
                    .unwrap_or("")
            })
            .or(self.deployer.deployment.as_ref())
    }

    /// Deployments, newest first.
    pub fn deployments_sorted(&self) -> Vec<&Deployment> {
        let mut deployments: Vec<&Deployment> = self.store.deployments.iter().collect();
        deployments.sort_by(|a, b| b.time_key().cmp(a.time_key()));
        deployments
    }

    /// Everything the Deployments page lists, newest first: deployments, plus
    /// generations that never became one (still in progress, failed, or
    /// skipped because their output was already deployed). Items that are in
    /// progress come first.
    pub fn history(&self) -> Vec<HistoryItem<'_>> {
        let mut items: Vec<HistoryItem<'_>> = Vec::new();
        let deployed_generation = |uuid: Option<&str>| {
            uuid.is_some_and(|uuid| {
                self.store
                    .deployments
                    .iter()
                    .chain(self.deployer.deployment.as_ref())
                    .any(|deployment| {
                        deployment
                            .generation
                            .as_ref()
                            .and_then(|g| g.uuid.as_deref())
                            == Some(uuid)
                    })
            })
        };

        for deployment in &self.store.deployments {
            items.push(HistoryItem::for_deployment(deployment, false));
        }
        if let Some(deployment) = &self.deployer.deployment
            && !self
                .store
                .deployments
                .iter()
                .any(|stored| stored.uuid == deployment.uuid)
        {
            items.push(HistoryItem::for_deployment(
                deployment,
                self.deployer.is_deploying.unwrap_or(false),
            ));
        } else if self.deployer.is_deploying.unwrap_or(false)
            && let Some(deployment) = &self.deployer.deployment
            && let Some(item) = items.iter_mut().find(|item| item.key == deployment.uuid)
        {
            item.active = true;
        }

        let builder_active = self.builder.is_evaluating.unwrap_or(false)
            || self.builder.is_building.unwrap_or(false);
        if let Some(generation) = &self.builder.generation
            && !deployed_generation(generation.uuid.as_deref())
        {
            items.push(HistoryItem::for_generation(generation, builder_active));
        }
        for generation in &self.store.generations {
            let is_builder_generation = generation.uuid.is_some()
                && self
                    .builder
                    .generation
                    .as_ref()
                    .and_then(|g| g.uuid.as_deref())
                    == generation.uuid.as_deref();
            if !is_builder_generation && !deployed_generation(generation.uuid.as_deref()) {
                items.push(HistoryItem::for_generation(generation, false));
            }
        }

        items.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then_with(|| b.time_key().cmp(a.time_key()))
        });
        items
    }

    pub fn can_switch_latest(&self) -> bool {
        if self.deployer.is_deploying.unwrap_or(false) {
            return false;
        }

        let Some(latest) = self.latest_deployment() else {
            return false;
        };

        let is_boot = latest.operation.as_deref() == Some("boot");
        let succeeded = latest.status.as_deref() == Some("done");
        let is_successful_retained = self
            .store
            .deployments_successful
            .iter()
            .any(|uuid| uuid == &latest.uuid);
        let not_already_switched = self.store.deployment_switched.as_deref() != Some(&latest.uuid);

        is_boot && succeeded && is_successful_retained && not_already_switched
    }

    pub fn can_retry_deployment(&self) -> bool {
        if self.deployer.is_deploying.unwrap_or(false)
            || self.builder.is_building.unwrap_or(false)
            || self.builder.is_evaluating.unwrap_or(false)
        {
            return false;
        }
        self.latest_deployment().is_some()
    }

    pub fn confirmation_needed(&self) -> bool {
        self.build_confirmer
            .as_ref()
            .is_some_and(Confirmer::is_pending)
            || self
                .deploy_confirmer
                .as_ref()
                .is_some_and(Confirmer::is_pending)
    }

    pub fn reboot_reason(&self) -> Option<&'static str> {
        if !self.need_to_reboot.unwrap_or(false) {
            return None;
        }
        if self
            .deployer
            .deployment
            .as_ref()
            .is_some_and(|deployment| deployment.operation.as_deref() == Some("switch"))
        {
            Some("The switch finished. Restart to use the new kernel.")
        } else {
            Some("Restart to use the latest deployment.")
        }
    }

    fn eval_started_relative(&self) -> Option<String> {
        self.builder
            .generation
            .as_ref()
            .and_then(|g| g.eval_started_at.as_deref())
            .map(format_relative_time_now)
    }

    fn build_started_relative(&self) -> Option<String> {
        self.builder
            .generation
            .as_ref()
            .and_then(|g| g.build_started_at.as_deref())
            .map(format_relative_time_now)
    }

    fn deploy_started_relative(&self) -> Option<String> {
        self.deployer
            .deployment
            .as_ref()
            .and_then(|d| d.started_at.as_deref())
            .map(format_relative_time_now)
    }

    fn failed(&self) -> bool {
        let generation_failed = self.builder.generation.as_ref().is_some_and(|generation| {
            generation.eval_status.as_deref() == Some("failed")
                || generation.build_status.as_deref() == Some("failed")
                || generation.eval_err.as_ref().is_some_and(|e| !e.is_empty())
                || generation.build_err.as_ref().is_some_and(|e| !e.is_empty())
        });
        let deployment_failed = self.latest_deployment().is_some_and(|deployment| {
            deployment.status.as_deref() == Some("failed")
                || deployment.error_msg.as_ref().is_some_and(|e| !e.is_empty())
        });
        let fetch_failed = self.fetcher.repository_status.as_ref().is_some_and(|repo| {
            repo.error_msg.as_ref().is_some_and(|e| !e.is_empty())
                || repo
                    .remotes
                    .iter()
                    .any(|r| r.fetch_error_msg.as_ref().is_some_and(|e| !e.is_empty()))
        });

        generation_failed || deployment_failed || fetch_failed
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Builder {
    pub is_evaluating: Option<bool>,
    pub is_building: Option<bool>,
    pub is_suspended: Option<bool>,
    pub hostname: Option<String>,
    pub generation: Option<Generation>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Deployer {
    pub is_deploying: Option<bool>,
    pub is_suspended: Option<bool>,
    pub deployment: Option<Deployment>,
    pub operation: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Fetcher {
    pub is_fetching: Option<bool>,
    pub repository_status: Option<RepositoryStatus>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RepositoryStatus {
    pub selected_commit_id: Option<String>,
    pub selected_commit_msg: Option<String>,
    pub selected_remote_name: Option<String>,
    pub selected_branch_name: Option<String>,
    pub error_msg: Option<String>,
    pub remotes: Vec<Remote>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub fetched: Option<bool>,
    pub fetched_at: Option<String>,
    pub fetch_error_msg: Option<String>,
    pub main: Option<Branch>,
    pub testing: Option<Branch>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Branch {
    pub name: Option<String>,
    pub commit_id: Option<String>,
    pub commit_msg: Option<String>,
    pub error_msg: Option<String>,
    pub on_top_of: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Store {
    pub deployments: Vec<Deployment>,
    pub generations: Vec<Generation>,
    pub deployment_switched: Option<String>,
    pub deployment_booted: Option<String>,
    pub deployments_boot_entry: Vec<String>,
    pub deployments_successful: Vec<String>,
    pub deployments_any: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Deployment {
    pub uuid: String,
    pub generation: Option<Generation>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: Option<String>,
    pub status: Option<String>,
    pub operation: Option<String>,
    pub operation_submitted: Option<String>,
    pub reason: Option<String>,
    pub profile_path: Option<String>,
    pub error_msg: Option<String>,
}

impl Deployment {
    /// The timestamp deployments are ordered by.
    pub fn time_key(&self) -> &str {
        self.ended_at
            .as_deref()
            .or(self.created_at.as_deref())
            .or(self.started_at.as_deref())
            .unwrap_or("")
    }

    pub fn is_switched(&self, store: &Store) -> bool {
        store.deployment_switched.as_deref() == Some(&self.uuid)
    }

    pub fn is_booted(&self, store: &Store) -> bool {
        store.deployment_booted.as_deref() == Some(&self.uuid)
    }

    pub fn is_boot_entry(&self, store: &Store) -> bool {
        store.deployments_boot_entry.iter().any(|u| u == &self.uuid)
    }

    pub fn is_successful(&self, store: &Store) -> bool {
        store.deployments_successful.iter().any(|u| u == &self.uuid)
    }

    pub fn commit_id(&self) -> Option<&str> {
        self.generation
            .as_ref()
            .and_then(|g| g.selected_commit_id.as_deref())
    }

    pub fn commit_msg(&self) -> Option<&str> {
        self.generation
            .as_ref()
            .and_then(|g| g.selected_commit_msg.as_deref())
    }

    #[allow(dead_code)]
    pub fn out_path(&self) -> Option<&str> {
        self.generation.as_ref().and_then(|g| g.out_path.as_deref())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Generation {
    pub uuid: Option<String>,
    pub hostname: Option<String>,
    pub selected_remote_name: Option<String>,
    pub selected_branch_name: Option<String>,
    pub selected_commit_id: Option<String>,
    pub selected_commit_msg: Option<String>,
    pub eval_status: Option<String>,
    pub eval_started_at: Option<String>,
    pub eval_ended_at: Option<String>,
    pub eval_err: Option<String>,
    pub drv_path: Option<String>,
    pub out_path: Option<String>,
    pub build_status: Option<String>,
    pub build_reason: Option<String>,
    pub build_started_at: Option<String>,
    pub build_ended_at: Option<String>,
    pub build_err: Option<String>,
}

/// One row of the Deployments page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryItem<'a> {
    /// The deployment uuid, or the generation uuid for a generation that
    /// was not deployed (`"current"` when Comin reports none).
    pub key: String,
    pub deployment: Option<&'a Deployment>,
    pub generation: Option<&'a Generation>,
    /// Comin is still evaluating, building or deploying this item.
    pub active: bool,
}

impl<'a> HistoryItem<'a> {
    fn for_deployment(deployment: &'a Deployment, active: bool) -> Self {
        Self {
            key: deployment.uuid.clone(),
            deployment: Some(deployment),
            generation: deployment.generation.as_ref(),
            active,
        }
    }

    fn for_generation(generation: &'a Generation, active: bool) -> Self {
        Self {
            key: generation
                .uuid
                .clone()
                .filter(|uuid| !uuid.is_empty())
                .unwrap_or_else(|| "current".into()),
            deployment: None,
            generation: Some(generation),
            active,
        }
    }

    /// Whether `uuid` names this item's deployment or generation.
    pub fn matches(&self, uuid: &str) -> bool {
        self.key == uuid || self.generation.and_then(|g| g.uuid.as_deref()) == Some(uuid)
    }

    pub fn time_key(&self) -> &str {
        if let Some(deployment) = self.deployment {
            return deployment.time_key();
        }
        self.generation
            .and_then(|g| {
                g.build_ended_at
                    .as_deref()
                    .or(g.eval_ended_at.as_deref())
                    .or(g.build_started_at.as_deref())
                    .or(g.eval_started_at.as_deref())
            })
            .unwrap_or("")
    }

    /// A short status: `done`, `failed`, `evaluating`, `not deployed`, …
    pub fn status(&self) -> &str {
        let generation = self.generation;
        let eval_failed = generation.is_some_and(|g| {
            g.eval_status.as_deref() == Some("failed")
                || g.eval_err.as_deref().is_some_and(|e| !e.is_empty())
        });
        let build_failed = generation.is_some_and(|g| {
            g.build_status.as_deref() == Some("failed")
                || g.build_err.as_deref().is_some_and(|e| !e.is_empty())
        });

        if let Some(deployment) = self.deployment {
            if self.active {
                return "deploying";
            }
            if deployment
                .error_msg
                .as_deref()
                .is_some_and(|e| !e.is_empty())
            {
                return "failed";
            }
            return deployment.status.as_deref().unwrap_or("unknown");
        }
        if eval_failed || build_failed {
            return "failed";
        }
        if self.active {
            return match generation.and_then(|g| g.build_started_at.as_deref()) {
                Some(_) => "building",
                None => "evaluating",
            };
        }
        "not deployed"
    }

    pub fn operation(&self) -> Option<&str> {
        self.deployment
            .and_then(|d| d.operation.as_deref().or(d.operation_submitted.as_deref()))
    }

    pub fn commit_id(&self) -> Option<&str> {
        self.generation
            .and_then(|g| g.selected_commit_id.as_deref())
    }

    pub fn commit_msg(&self) -> Option<&str> {
        self.generation
            .and_then(|g| g.selected_commit_msg.as_deref())
    }

    /// The journal window that holds this item's logs: from the start of the
    /// evaluation to the end of the deployment, or open-ended while active.
    pub fn log_window(&self) -> Option<(OffsetDateTime, Option<OffsetDateTime>)> {
        let generation = self.generation;
        let deployment = self.deployment;
        let start = generation
            .and_then(|g| {
                g.eval_started_at
                    .as_deref()
                    .or(g.build_started_at.as_deref())
            })
            .or_else(|| {
                deployment.and_then(|d| d.started_at.as_deref().or(d.created_at.as_deref()))
            })
            .and_then(parse_rfc3339)?;
        let since = start - Duration::seconds(2);

        if self.active {
            return Some((since, None));
        }
        let end = deployment
            .and_then(|d| d.ended_at.as_deref())
            .or_else(|| {
                generation.and_then(|g| g.build_ended_at.as_deref().or(g.eval_ended_at.as_deref()))
            })
            .and_then(parse_rfc3339)
            .unwrap_or(start);
        Some((since, Some(end + Duration::seconds(3))))
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Confirmer {
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub mode: Option<String>,
    pub submitted: Option<String>,
    pub confirmed: Option<String>,
    pub autoconfirm_started: Option<bool>,
    pub autoconfirm_started_at: Option<String>,
}

impl Confirmer {
    pub fn is_pending(&self) -> bool {
        if let Some(submitted) = self.submitted.as_deref().filter(|s| !s.is_empty()) {
            return self.confirmed.as_deref() != Some(submitted);
        }
        false
    }
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    Ok(v.map(|val| match val {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_phase_takes_precedence_over_reboot_required() {
        let state = CominState {
            need_to_reboot: Some(true),
            builder: Builder {
                is_building: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state.phase(), Phase::Building);

        let state_eval = CominState {
            need_to_reboot: Some(true),
            builder: Builder {
                is_evaluating: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state_eval.phase(), Phase::Evaluating);

        let state_deploy = CominState {
            need_to_reboot: Some(true),
            deployer: Deployer {
                is_deploying: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state_deploy.phase(), Phase::Deploying);

        let state_fetch = CominState {
            need_to_reboot: Some(true),
            fetcher: Fetcher {
                is_fetching: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state_fetch.phase(), Phase::Fetching);
    }

    #[test]
    fn suspended_takes_precedence_over_active_work() {
        let state = CominState {
            is_suspended: Some(true),
            need_to_reboot: Some(true),
            builder: Builder {
                is_building: Some(true),
                ..Default::default()
            },
            deployer: Deployer {
                is_deploying: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state.phase(), Phase::Suspended);
    }

    #[test]
    fn failed_eval_produces_failed() {
        let state = CominState {
            builder: Builder {
                generation: Some(Generation {
                    eval_status: Some("failed".into()),
                    eval_err: Some("syntax error".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state.phase(), Phase::Failed);
    }

    #[test]
    fn failed_build_produces_failed() {
        let state = CominState {
            builder: Builder {
                generation: Some(Generation {
                    build_status: Some("failed".into()),
                    build_err: Some("derivation failed".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state.phase(), Phase::Failed);
    }

    #[test]
    fn failed_deployment_produces_failed() {
        let state = CominState {
            deployer: Deployer {
                deployment: Some(Deployment {
                    uuid: "d-1".into(),
                    status: Some("failed".into()),
                    error_msg: Some("activation error".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(state.phase(), Phase::Failed);
    }

    #[test]
    fn live_switch_eligibility_rules() {
        let mut deployment = Deployment {
            uuid: "d-boot".into(),
            status: Some("done".into()),
            operation: Some("boot".into()),
            created_at: Some("2026-01-02T00:00:00Z".into()),
            ..Default::default()
        };

        let mut state = CominState {
            store: Store {
                deployments: vec![deployment.clone()],
                deployment_switched: Some("d-prev".into()),
                deployments_successful: vec!["d-boot".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        // Meets all 5 conditions:
        // 1. latest deployment is boot
        // 2. deployment succeeded ("done")
        // 3. is retained as successful
        // 4. not already switched
        // 5. no deployment currently active
        assert!(state.can_switch_latest());

        // Fails condition 1: operation is switch instead of boot
        deployment.operation = Some("switch".into());
        state.store.deployments = vec![deployment.clone()];
        assert!(!state.can_switch_latest());

        // Fails condition 2: deployment failed
        deployment.operation = Some("boot".into());
        deployment.status = Some("failed".into());
        state.store.deployments = vec![deployment.clone()];
        assert!(!state.can_switch_latest());

        // Fails condition 3: not in deployments_successful
        deployment.status = Some("done".into());
        state.store.deployments = vec![deployment.clone()];
        state.store.deployments_successful = vec![];
        assert!(!state.can_switch_latest());

        // Fails condition 4: already switched
        state.store.deployments_successful = vec!["d-boot".into()];
        state.store.deployment_switched = Some("d-boot".into());
        assert!(!state.can_switch_latest());

        // Fails condition 5: currently deploying
        state.store.deployment_switched = Some("d-prev".into());
        state.deployer.is_deploying = Some(true);
        assert!(!state.can_switch_latest());
    }

    #[test]
    fn confirmation_pending_detection() {
        let mut state = CominState {
            build_confirmer: Some(Confirmer {
                mode: Some("manual".into()),
                submitted: Some("gen-1".into()),
                confirmed: Some("".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(state.confirmation_needed());

        // Once confirmed matches submitted, confirmation is no longer needed
        state.build_confirmer.as_mut().unwrap().confirmed = Some("gen-1".into());
        assert!(!state.confirmation_needed());
    }

    #[test]
    fn tooltip_never_contains_selected_commit_msg() {
        let long_multiline_msg = "feat: exciting new feature\n\nThis is a 500-line release note.\nFixes #123.\nDetailed explanations...";
        let state = CominState {
            builder: Builder {
                hostname: Some("Ed-PCL".into()),
                ..Default::default()
            },
            fetcher: Fetcher {
                repository_status: Some(RepositoryStatus {
                    selected_commit_id: Some("c8aaa7d6f168fc8ac8d190a356eab6ed1a40a22b".into()),
                    selected_commit_msg: Some(long_multiline_msg.into()),
                    selected_remote_name: Some("github".into()),
                    selected_branch_name: Some("deploy".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            need_to_reboot: Some(true),
            ..Default::default()
        };

        let tray_state = TrayState::from_status(state);
        let tip = tray_state.tooltip();

        assert!(!tip.contains("exciting new feature"));
        assert!(!tip.contains("500-line release note"));
        assert!(!tip.contains("Detailed explanations"));
        assert_eq!(tip, "Ed-PCL · github/deploy @ c8aaa7d6");
    }

    #[test]
    fn tooltip_stays_concise_during_operation_and_failure() {
        // Building state
        let building_state = CominState {
            builder: Builder {
                hostname: Some("Ed-PCL".into()),
                is_building: Some(true),
                generation: Some(Generation {
                    selected_commit_id: Some("c8aaa7d6".into()),
                    selected_remote_name: Some("github".into()),
                    selected_branch_name: Some("deploy".into()),
                    build_started_at: Some("2026-09-21T22:20:00Z".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let tip = TrayState::from_status(building_state).tooltip();
        let lines: Vec<&str> = tip.lines().collect();
        assert!(lines.len() <= 2);
        assert!(lines[0].contains("Ed-PCL · github/deploy @ c8aaa7d6"));
        assert!(lines[1].starts_with("Build started"));

        // Failure state
        let failed_state = CominState {
            builder: Builder {
                generation: Some(Generation {
                    eval_status: Some("failed".into()),
                    eval_err: Some("A very long raw error stack trace...".into()),
                    selected_commit_id: Some("c8aaa7d6".into()),
                    selected_remote_name: Some("github".into()),
                    selected_branch_name: Some("deploy".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let tip_failed = TrayState::from_status(failed_state).tooltip();
        assert!(!tip_failed.contains("very long raw error"));
        let fail_lines: Vec<&str> = tip_failed.lines().collect();
        assert_eq!(fail_lines.len(), 2);
        assert_eq!(fail_lines[0], "github/deploy @ c8aaa7d6");
        assert_eq!(fail_lines[1], "Open Comin for details");
    }

    #[test]
    fn parse_status_normal_fixture() {
        let json = include_str!("../tests/fixtures/status_normal.json");
        let state: CominState = serde_json::from_str(json).expect("valid json");

        assert_eq!(state.hostname(), Some("Ed-PCL"));
        assert_eq!(state.phase(), Phase::RebootRequired);
        assert!(!state.is_suspended.unwrap());
        assert!(state.need_to_reboot.unwrap());

        // Check fetcher & remotes
        let repo = state.fetcher.repository_status.as_ref().unwrap();
        assert_eq!(repo.remotes.len(), 1);
        let remote = &repo.remotes[0];
        assert_eq!(remote.name, "github");
        assert_eq!(
            remote.url,
            "https://github.com/JuanDelPueblo/nix-config.git"
        );
        assert_eq!(
            remote.main.as_ref().unwrap().name.as_deref(),
            Some("deploy")
        );
        assert_eq!(
            remote.testing.as_ref().unwrap().error_msg.as_deref(),
            Some("the branch 'github/testing-Ed-PCL' doesn't exist")
        );

        // Check latest deployment & retention roles
        let latest = state.latest_deployment().unwrap();
        assert_eq!(latest.uuid, "c2483f11-e0e1-4447-81be-d58456f06e63");
        assert_eq!(latest.operation.as_deref(), Some("boot"));
        assert_eq!(latest.status.as_deref(), Some("done"));
        assert!(latest.is_boot_entry(&state.store));
        assert!(latest.is_successful(&state.store));
        assert!(!latest.is_switched(&state.store));

        // Switch live is eligible
        assert!(state.can_switch_latest());
    }

    #[test]
    fn parse_status_evaluating_fixture() {
        let json = include_str!("../tests/fixtures/status_evaluating.json");
        let state: CominState = serde_json::from_str(json).expect("valid json");
        assert_eq!(state.phase(), Phase::Evaluating);
    }

    #[test]
    fn parse_status_failed_eval_fixture() {
        let json = include_str!("../tests/fixtures/status_failed_eval.json");
        let state: CominState = serde_json::from_str(json).expect("valid json");
        assert_eq!(state.phase(), Phase::Failed);
    }

    #[test]
    fn parse_status_confirmation_needed_fixture() {
        let json = include_str!("../tests/fixtures/status_confirmation_needed.json");
        let state: CominState = serde_json::from_str(json).expect("valid json");
        assert!(state.confirmation_needed());
    }

    #[test]
    fn parse_status_suspended_fixture() {
        let json = include_str!("../tests/fixtures/status_suspended.json");
        let state: CominState = serde_json::from_str(json).expect("valid json");
        assert_eq!(state.phase(), Phase::Suspended);
    }
}
