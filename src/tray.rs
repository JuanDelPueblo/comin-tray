use ksni::Tray;
use tokio::sync::mpsc;

use crate::model::TrayState;

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Fetch,
    Suspend,
    Resume,
    SwitchLatest,
    Quit,
}

pub struct CominTray {
    pub state: TrayState,
    pub action_tx: mpsc::Sender<Action>,
}

impl Tray for CominTray {
    fn id(&self) -> String {
        "comin-tray".into()
    }

    fn title(&self) -> String {
        format!("Comin — {}", self.state.phase.label())
    }

    fn icon_name(&self) -> String {
        self.state.phase.icon_name().into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: self.icon_name(),
            title: self.title(),
            description: self.state.tooltip(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let mut items = vec![ksni::MenuItem::Standard(ksni::menu::StandardItem {
            label: self.title(),
            enabled: false,
            ..Default::default()
        })];

        if let Some(status) = &self.state.status {
            if let Some(repository) = status.fetcher.repository_status.as_ref() {
                let commit: String = repository.selected_commit_id.chars().take(8).collect();
                items.push(ksni::MenuItem::Standard(ksni::menu::StandardItem {
                    label: format!(
                        "{}/{} @ {}",
                        repository.selected_remote_name, repository.selected_branch_name, commit
                    ),
                    enabled: false,
                    ..Default::default()
                }));
            }

            if let Some(deployment) = status.latest_deployment() {
                items.push(ksni::MenuItem::Standard(ksni::menu::StandardItem {
                    label: format!("Latest: {} {}", deployment.operation, deployment.status),
                    enabled: false,
                    ..Default::default()
                }));
            }
        } else if let Some(error) = &self.state.error {
            items.push(ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: error.clone(),
                enabled: false,
                ..Default::default()
            }));
        }

        items.push(ksni::MenuItem::Separator);
        items.push(action_item("Fetch now", "view-refresh", Action::Fetch));

        if self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.is_suspended.unwrap_or(false))
        {
            items.push(action_item(
                "Resume",
                "media-playback-start",
                Action::Resume,
            ));
        } else {
            items.push(action_item(
                "Suspend",
                "media-playback-pause",
                Action::Suspend,
            ));
        }

        if self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.can_switch_latest())
        {
            items.push(action_item(
                "Activate latest with switch",
                "system-software-update",
                Action::SwitchLatest,
            ));
        }

        items.push(ksni::MenuItem::Separator);
        items.push(action_item("Quit", "application-exit", Action::Quit));
        items
    }
}

fn action_item(label: &str, icon: &str, action: Action) -> ksni::MenuItem<CominTray> {
    ksni::MenuItem::Standard(ksni::menu::StandardItem {
        label: label.into(),
        icon_name: icon.into(),
        activate: Box::new(move |tray: &mut CominTray| {
            let _ = tray.action_tx.try_send(action);
        }),
        ..Default::default()
    })
}
