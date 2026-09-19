use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;

pub fn open() -> Result<()> {
    let mut child = Command::new("xdg-terminal-exec")
        .args([
            "--title=Comin logs",
            "--",
            "journalctl",
            "--unit=comin.service",
            "--follow",
            "--lines=100",
            "--no-pager",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("could not open the Comin logs")?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    Ok(())
}
