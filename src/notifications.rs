use std::collections::HashMap;

use zbus::{Connection, proxy, zvariant::Value};

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

pub async fn send(summary: &str, body: &str, icon: &str) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let proxy = NotificationsProxy::new(&connection).await?;
    proxy
        .notify(
            "Comin Tray",
            0,
            icon,
            summary,
            body,
            &[],
            HashMap::new(),
            5000,
        )
        .await?;
    Ok(())
}
