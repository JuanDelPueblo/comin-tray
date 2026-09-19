use std::collections::HashSet;

use serde::Deserialize;

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
                .clone()
                .unwrap_or_else(|| "Comin is unavailable".into());
        };

        let mut lines = Vec::new();
        if let Some(repository) = status.fetcher.repository_status.as_ref() {
            let commit = short_commit(&repository.selected_commit_id);
            lines.push(format!(
                "Source: {}/{} @ {}",
                value_or_unknown(&repository.selected_remote_name),
                value_or_unknown(&repository.selected_branch_name),
                value_or_unknown(&commit)
            ));
            if !repository.selected_commit_msg.is_empty() {
                lines.push(format!("Commit: {}", repository.selected_commit_msg));
            }
        } else {
            lines.push("Source: unknown".into());
        }

        if let Some(deployment) = status.latest_deployment() {
            let commit = short_commit(&deployment.generation.selected_commit_id);
            lines.push(format!(
                "Latest deployment: {} {} @ {}",
                value_or_unknown(&deployment.operation),
                value_or_unknown(&deployment.status),
                value_or_unknown(&commit)
            ));
        } else {
            lines.push("Latest deployment: none".into());
        }

        lines.join("\n")
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CominState {
    pub need_to_reboot: Option<bool>,
    pub is_suspended: Option<bool>,
    pub builder: Builder,
    pub deployer: Deployer,
    pub fetcher: Fetcher,
    pub store: Store,
}

impl CominState {
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

    pub fn latest_deployment(&self) -> Option<&Deployment> {
        self.store
            .deployments
            .iter()
            .max_by_key(|deployment| {
                deployment
                    .ended_at
                    .as_deref()
                    .or(deployment.created_at.as_deref())
                    .unwrap_or("")
            })
            .or(self.deployer.deployment.as_ref())
    }

    pub fn can_switch_latest(&self) -> bool {
        if self.deployer.operation != "boot" || self.deployer.is_deploying.unwrap_or(false) {
            return false;
        }

        let Some(latest) = self.latest_deployment() else {
            return false;
        };
        let successful: HashSet<&str> = self
            .store
            .deployments_successful
            .iter()
            .map(String::as_str)
            .collect();

        latest.status == "done"
            && successful.contains(latest.uuid.as_str())
            && self.store.deployment_switched != latest.uuid
    }

    fn failed(&self) -> bool {
        let generation_failed = self.builder.generation.as_ref().is_some_and(|generation| {
            generation.eval_status == "failed" || generation.build_status == "failed"
        });
        let deployment_failed = self
            .latest_deployment()
            .is_some_and(|deployment| deployment.status == "failed");

        generation_failed || deployment_failed
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Builder {
    pub is_evaluating: Option<bool>,
    pub is_building: Option<bool>,
    pub generation: Option<Generation>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Deployer {
    pub is_deploying: Option<bool>,
    pub deployment: Option<Deployment>,
    pub operation: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Fetcher {
    pub is_fetching: Option<bool>,
    pub repository_status: Option<RepositoryStatus>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RepositoryStatus {
    pub selected_commit_id: String,
    pub selected_commit_msg: String,
    pub selected_remote_name: String,
    pub selected_branch_name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Store {
    pub deployments: Vec<Deployment>,
    pub deployment_switched: String,
    pub deployments_successful: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Deployment {
    pub uuid: String,
    pub generation: Generation,
    pub ended_at: Option<String>,
    pub created_at: Option<String>,
    pub status: String,
    pub operation: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Generation {
    pub selected_commit_id: String,
    pub eval_status: String,
    pub build_status: String,
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(8).collect()
}

fn value_or_unknown(value: &str) -> &str {
    if value.is_empty() { "unknown" } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_phase_has_priority_over_reboot() {
        let state = CominState {
            need_to_reboot: Some(true),
            builder: Builder {
                is_building: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(state.phase(), Phase::Building);
    }

    #[test]
    fn live_switch_requires_latest_successful_boot_deployment() {
        let latest = Deployment {
            uuid: "new".into(),
            status: "done".into(),
            operation: "boot".into(),
            created_at: Some("2026-01-02T00:00:00Z".into()),
            ..Default::default()
        };
        let state = CominState {
            deployer: Deployer {
                operation: "boot".into(),
                ..Default::default()
            },
            store: Store {
                deployments: vec![latest],
                deployment_switched: "old".into(),
                deployments_successful: vec!["new".into()],
            },
            ..Default::default()
        };

        assert!(state.can_switch_latest());
    }

    #[test]
    fn status_json_uses_the_v014_field_names() {
        let state: CominState = serde_json::from_str(
            r#"{
                "need_to_reboot": true,
                "is_suspended": false,
                "builder": {"is_evaluating": false, "is_building": false},
                "deployer": {"is_deploying": false, "operation": "boot"},
                "fetcher": {
                    "is_fetching": false,
                    "repository_status": {
                        "selected_commit_id": "1234567890",
                        "selected_remote_name": "github",
                        "selected_branch_name": "deploy"
                    }
                },
                "store": {"deployments": [], "deployments_successful": []}
            }"#,
        )
        .unwrap();

        assert_eq!(state.phase(), Phase::RebootRequired);
        assert!(
            TrayState::from_status(state)
                .tooltip()
                .contains("github/deploy @ 12345678")
        );
    }
}
