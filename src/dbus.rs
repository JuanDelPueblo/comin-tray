//! Session-bus helpers: single-instance names for the tray and the GUI, the
//! GUI's `Show` method, and the checks behind `comin-tray diagnose`.

use anyhow::{Context, Result};
use zbus::{
    Connection, fdo::DBusProxy, fdo::RequestNameFlags, fdo::RequestNameReply, interface,
    names::BusName,
};

pub const TRAY_NAME: &str = "io.github.JuanDelPueblo.CominTray";
pub const GUI_NAME: &str = "io.github.JuanDelPueblo.CominTray.Gui";
pub const GUI_PATH: &str = "/io/github/JuanDelPueblo/CominTray/Gui";

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";

/// Claims the tray's well-known name. Returns the connection that holds the
/// name, or `None` when another tray already owns it.
pub async fn claim_tray_name() -> Result<Option<Connection>> {
    let connection = Connection::session()
        .await
        .context("Could not connect to the session bus")?;
    let reply = connection
        .request_name_with_flags(TRAY_NAME, RequestNameFlags::DoNotQueue.into())
        .await
        .context("Could not request the tray's D-Bus name")?;
    Ok((reply == RequestNameReply::PrimaryOwner).then_some(connection))
}

/// Asks a running GUI to show `page`. Returns `Ok(false)` when no GUI runs.
pub async fn show_running_gui(page: &str) -> Result<bool> {
    let connection = Connection::session().await?;
    if !name_has_owner(&connection, GUI_NAME).await? {
        return Ok(false);
    }
    connection
        .call_method(Some(GUI_NAME), GUI_PATH, Some(GUI_NAME), "Show", &(page,))
        .await
        .context("The running Comin window did not answer")?;
    Ok(true)
}

/// The object a running GUI exports so that other processes can raise it.
pub struct GuiService {
    pub pages: iced::futures::channel::mpsc::Sender<String>,
}

#[interface(name = "io.github.JuanDelPueblo.CominTray.Gui")]
impl GuiService {
    async fn show(&self, page: String) {
        let mut pages = self.pages.clone();
        let _ = iced::futures::SinkExt::send(&mut pages, page).await;
    }
}

/// Exports [`GuiService`] and claims the GUI name. The returned connection
/// must be kept alive for as long as the GUI runs.
pub async fn serve_gui(service: GuiService) -> Result<Connection> {
    zbus::connection::Builder::session()?
        .serve_at(GUI_PATH, service)?
        .name(GUI_NAME)?
        .build()
        .await
        .context("Could not export the Comin window on the session bus")
}

async fn name_has_owner(connection: &Connection, name: &str) -> Result<bool> {
    let proxy = DBusProxy::new(connection).await?;
    Ok(proxy.name_has_owner(BusName::try_from(name)?).await?)
}

/// One line of `comin-tray diagnose` output.
pub struct Check {
    pub ok: bool,
    pub label: String,
    pub detail: String,
}

impl Check {
    fn new(ok: bool, label: &str, detail: impl Into<String>) -> Self {
        Self {
            ok,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

/// Runs the session-bus checks of `comin-tray diagnose`.
pub async fn session_checks() -> Vec<Check> {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            return vec![Check::new(
                false,
                "Session bus",
                format!("cannot connect: {error}"),
            )];
        }
    };
    let mut checks = vec![Check::new(true, "Session bus", "connected")];

    let watcher = name_has_owner(&connection, WATCHER_NAME)
        .await
        .unwrap_or(false);
    checks.push(Check::new(
        watcher,
        "StatusNotifierWatcher",
        if watcher {
            "present"
        } else {
            "missing: the desktop has no tray (SNI) support running yet"
        },
    ));

    if watcher {
        let host = watcher_host_registered(&connection).await;
        checks.push(match host {
            Ok(true) => Check::new(true, "StatusNotifierHost", "registered"),
            Ok(false) => Check::new(
                false,
                "StatusNotifierHost",
                "not registered: no panel is showing tray icons",
            ),
            Err(error) => Check::new(false, "StatusNotifierHost", error.to_string()),
        });
    }

    let tray = name_has_owner(&connection, TRAY_NAME)
        .await
        .unwrap_or(false);
    checks.push(Check::new(
        tray,
        "comin-tray daemon",
        if tray {
            "running"
        } else {
            "not running: start `comin-tray tray` (see README, Autostart)"
        },
    ));

    let notifications = name_has_owner(&connection, NOTIFICATIONS_NAME)
        .await
        .unwrap_or(false);
    checks.push(Check::new(
        notifications,
        "Notifications",
        if notifications {
            "present"
        } else {
            "missing: desktop notifications will not show"
        },
    ));

    checks
}

async fn watcher_host_registered(connection: &Connection) -> Result<bool> {
    let proxy = zbus::Proxy::new(connection, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME).await?;
    Ok(proxy
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .await?)
}
