use std::{ffi::OsStr, process::Stdio, time::Duration};

use anyhow::{Context, Result, bail};
use tokio::{process::Command, time::timeout};

use crate::model::CominState;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct CominClient {
    executable: String,
}

impl Default for CominClient {
    fn default() -> Self {
        Self {
            executable: std::env::var("COMIN_TRAY_COMIN").unwrap_or_else(|_| "comin".into()),
        }
    }
}

impl CominClient {
    pub async fn status(&self) -> Result<CominState> {
        let output = self.run(["status", "--json"]).await?;
        serde_json::from_slice(&output).context("Comin returned invalid status JSON")
    }

    pub async fn fetch(&self) -> Result<()> {
        self.run(["fetch"]).await.map(|_| ())
    }

    pub async fn suspend(&self) -> Result<()> {
        self.run(["suspend"]).await.map(|_| ())
    }

    pub async fn resume(&self) -> Result<()> {
        self.run(["resume"]).await.map(|_| ())
    }

    pub async fn switch_latest(&self) -> Result<()> {
        self.run(["deployment", "submit-latest", "--operation", "switch"])
            .await
            .map(|_| ())
    }

    async fn run<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = timeout(COMMAND_TIMEOUT, command.output())
            .await
            .context("The Comin command timed out")?
            .with_context(|| format!("Failed to start {}", self.executable))?;

        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if message.is_empty() {
                bail!("The Comin command failed with {}", output.status);
            }
            bail!("{message}");
        }

        Ok(output.stdout)
    }
}
