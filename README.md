# Comin Desktop & Tray

A native desktop frontend and system tray indicator for [Comin](https://github.com/nlewo/comin) GitOps on NixOS and KDE Plasma.

It combines the concise live-state model of `comin watch` with the structured lifecycle presentation and retention insights of Cockpit NixOS Manager, providing an interactive dashboard and lightweight system tray companion.

## Features

- **Full Native GUI Dashboard**: Built with Iced, featuring an Overview dashboard and live streaming service logs.
- **5-Stage Lifecycle Overview**: Tracks Git Source (`remote/branch @ commit`), Fetch, Evaluation, Build, and Deployment in a structured, glanceable layout.
- **Detailed Component Cards**:
  - **Fetcher**: Configured remotes, fetch timestamps, and status for `main` and `testing` branches.
  - **Builder**: Commit ID, evaluation status/errors, build reason, derivation path, build errors, and output store path.
  - **Deployer**: Operation (`switch`, `boot`), status, submitted operation, reasons, profile path, and error messages.
- **Recent Deployments & Retention Table**: Lists past deployments with relative timestamps, operations, commit references, and retention badges (`switched`, `booted`, `boot entry`, `successful`).
- **Interactive Action Bar**:
  - Fetch now
  - Suspend / Resume GitOps
  - Switch live now (conditionally available when the latest deployment booted successfully, is retained, and hasn't been switched live yet)
  - Retry deployment (conditionally available when an existing deployment can be retried)
  - Accept confirmation (conditionally available when a build or deployment confirmer is pending)
- **Live Streaming Logs Tab**: Streams Comin systemd journal output with logfmt/JSON parsing, severity coloring, auto-scroll toggle, and clear buffer action.
- **Lean System Tray Daemon**: Uses `ksni` for StatusNotifierItem and `zbus` for notifications:
  - Left click activates and raises the Comin GUI Overview.
  - Clean right-click menu with interactive actions only (`Open Comin`, `Fetch now`, `Suspend/Resume GitOps`, `Switch live now`, `Accept confirmation`, `Retry latest deployment`, `View live logs`, `Quit`).
  - Concise 1–2 line tooltip (`hostname · remote/branch @ short_commit` and current activity/relative start time). Never dumps commit bodies, error traces, or long store paths into desktop tooltips.
  - Does **not** initialize GUI or windowing libraries in the persistent daemon process, preventing ghost taskbar entries in KDE Plasma / KWin.

## Usage & CLI

Launch the Comin GUI dashboard:

```console
comin-tray gui
```

Launch directly into the live logs tab:

```console
comin-tray gui logs
# or using the shortcut alias:
comin-tray logs
```

Run the persistent system tray daemon:

```console
comin-tray tray
# or without arguments:
comin-tray
```

Print command-line help:

```console
comin-tray --help
```

## Desktop Integration

The package installs a desktop entry `data/comin-tray.desktop` (`Name=Comin`, `GenericName=Comin GitOps Manager`, `Exec=comin-tray gui`) so Comin appears in application menus and application runners (KRunner, Rofi, etc.).

## Development

```console
nix develop
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
nix flake check
nix build
```
