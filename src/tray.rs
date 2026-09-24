use ksni::Tray;
use tokio::sync::mpsc;

use crate::model::TrayState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiPage {
    Overview,
    Deployments,
    Logs,
}

impl GuiPage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Deployments => "deployments",
            Self::Logs => "logs",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "overview" => Some(Self::Overview),
            "deployments" => Some(Self::Deployments),
            "logs" => Some(Self::Logs),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    OpenGui(GuiPage),
    Fetch,
    Suspend,
    Resume,
    SwitchLatest,
    AcceptConfirmation,
    RetryLatest,
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

    fn watcher_online(&self) {
        eprintln!("comin-tray: StatusNotifierWatcher is online; the icon is registered");
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        // Keep running: Plasma starts the watcher after autostart entries at
        // login and restarts it with plasmashell. ksni registers the icon
        // again as soon as the watcher comes back.
        eprintln!("comin-tray: StatusNotifierWatcher is offline ({reason:?}); waiting for it");
        true
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.action_tx.try_send(Action::OpenGui(GuiPage::Overview));
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let mut items = vec![action_item(
            "Open Comin",
            "system-software-update",
            Action::OpenGui(GuiPage::Overview),
        )];

        items.push(ksni::MenuItem::Separator);
        items.push(action_item("Fetch now", "view-refresh", Action::Fetch));

        if self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.is_suspended.unwrap_or(false))
        {
            items.push(action_item(
                "Resume GitOps",
                "media-playback-start",
                Action::Resume,
            ));
        } else {
            items.push(action_item(
                "Suspend GitOps",
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
                "Activate latest live",
                "system-software-update",
                Action::SwitchLatest,
            ));
        }

        if self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.confirmation_needed())
        {
            items.push(action_item(
                "Accept confirmation",
                "dialog-ok-apply",
                Action::AcceptConfirmation,
            ));
        }

        if self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.can_retry_deployment())
        {
            items.push(action_item(
                "Retry latest deployment",
                "view-refresh",
                Action::RetryLatest,
            ));
        }

        items.push(ksni::MenuItem::Separator);
        items.push(action_item(
            "Deployments…",
            "view-list-details",
            Action::OpenGui(GuiPage::Deployments),
        ));
        items.push(action_item(
            "View live logs",
            "utilities-terminal",
            Action::OpenGui(GuiPage::Logs),
        ));
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
