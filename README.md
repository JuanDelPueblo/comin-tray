# Comin Desktop & Tray

A native desktop frontend and system tray indicator for [Comin](https://github.com/nlewo/comin) GitOps on NixOS and KDE Plasma.

It combines the concise live-state model of `comin watch` with the structured lifecycle presentation and retention insights of Cockpit NixOS Manager, providing an interactive dashboard and lightweight system tray companion.

## Features

- **Full Native GUI**: Built with Iced, with three pages: Overview, Deployments and Logs.
- **5-Stage Lifecycle Overview**: Tracks Git Source (`remote/branch @ commit`), Fetch, Evaluation, Build, and Deployment in a structured, glanceable layout.
- **Detailed Component Cards**:
  - **Fetcher**: Configured remotes, fetch timestamps, and status for `main` and `testing` branches.
  - **Builder**: Commit ID, evaluation status/errors, build reason, derivation path, build errors, and output store path.
  - **Deployer**: Operation (`switch`, `boot`), status, submitted operation, reasons, profile path, and error messages.
- **Deployments page**: Every deployment Comin still remembers, plus generations that never became one (in progress, failed, or skipped because the same output was already deployed). Selecting one shows:
  - its commit, branch, operation, retention badges (`switched`, `booted`, `boot entry`, `successful`), and store and profile paths, each with a **Copy** button;
  - an `nh`-style stage timeline: **Evaluate → Build → Deploy**, with a duration and a result for each stage (for example `27 built · 13 fetched (3.2 GiB unpacked)`). It updates live while Comin works;
  - the list of derivations nix built and the paths it fetched, and from which cache;
  - that deployment's own log, read from the journal and filterable by stage.
- **Interactive Action Bar**:
  - Fetch now
  - Suspend / Resume GitOps
  - Switch live now (shown when the latest deployment booted successfully, is retained, and isn't live yet)
  - Retry deployment (shown when an existing deployment can be retried)
  - Accept confirmation (shown when a build or deployment confirmer is waiting)
- **Readable, copyable logs** (Logs page and each deployment's log):
  - Times in local time. Comin's `level=… msg=…` lines are unwrapped. nix's colored output and git's progress output are decoded.
  - **Hide noise** (on by default) hides git plumbing (`remote: …`, `[5K blob data]`, `fatal: Refusing to point HEAD…`), Comin's store bookkeeping and `structuredAttrs is enabled` chatter. Long `nix …` commands are shortened for display; copies keep the full text.
  - Click a line to select it. Shift+click selects a range, or turn on **Select range** to extend the selection with plain clicks. Ctrl+A selects all. Copy with Ctrl+C or **Copy selected**. **Copy all shown** and **Save to file** (saved to your Downloads folder) are there too. Long lines end with `…` on screen; copies keep the full text.
  - Search, a minimum-level filter, and **Open deployment** for a selected line that names a generation or deployment.
  - The live stream reconnects by itself if `journalctl` stops.
- **Lean System Tray Daemon**: Uses `ksni` for StatusNotifierItem and `zbus` for notifications:
  - Left click opens the Comin window. When it is already open, the window comes to the front instead.
  - Right-click menu with actions only (`Open Comin`, `Fetch now`, `Suspend/Resume GitOps`, `Activate latest live`, `Accept confirmation`, `Retry latest deployment`, `Deployments…`, `View live logs`, `Quit`).
  - Concise 1–2 line tooltip (`hostname · remote/branch @ short_commit` and current activity/relative start time).
  - Waits for the desktop's tray host instead of exiting when it starts first (common with XDG autostart at login), and registers again when the panel restarts.
  - Only one tray runs per session, even if it is started twice.
  - Does **not** initialize GUI or windowing libraries in the persistent daemon process, preventing ghost taskbar entries in KDE Plasma / KWin.

## Usage & CLI

```console
comin-tray gui               # open the Comin window (Overview)
comin-tray gui deployments   # …on the Deployments page (or: comin-tray deployments)
comin-tray gui logs          # …on the Logs page (or: comin-tray logs)
comin-tray                   # run the tray daemon (same as: comin-tray tray)
comin-tray diagnose          # check tray host, notifications, comin and journal access
comin-tray --help
```

Only one window opens per session. Launching it again brings the open window to the front on the requested page.

## Desktop Integration

The package installs a desktop entry `data/comin-tray.desktop` (`Name=Comin`, `GenericName=Comin GitOps Manager`, `Exec=comin-tray gui`) so Comin appears in application menus and application runners (KRunner, Rofi, etc.).

### Starting the tray at login

Pick **one** of these (a second copy exits by itself, so doing both is harmless):

- **home-manager (recommended)**: the flake exports a module that runs the tray as a systemd user service tied to `graphical-session.target` and restarts it on failure:

  ```nix
  {
    imports = [ comin-tray.homeManagerModules.default ];
    services.comin-tray.enable = true;
  }
  ```

  Its output is in `journalctl --user -u comin-tray`.

- **XDG autostart**: the package ships `share/comin-tray/comin-tray-autostart.desktop` (`Exec=comin-tray tray`, started after the Plasma panel). Link it into `~/.config/autostart/`, for example with home-manager's `xdg.autostart.entries`. Plasma runs it as a `app-comin*` user unit, so its output is in `journalctl --user -u 'app-comin*'`.

### The tray icon does not appear

Run `comin-tray diagnose`. It reports whether:

- a StatusNotifierWatcher and host (the panel's tray) are running;
- a comin-tray daemon is running;
- desktop notifications are available;
- `comin status` works;
- your user can read the `comin.service` journal. If it can't, add your user to the `systemd-journal` group; otherwise the Logs and Deployments pages stay empty.

## Testing against recorded data

`COMIN_TRAY_COMIN` and `COMIN_TRAY_JOURNALCTL` replace the `comin` and `journalctl` executables. Point them at scripts that print a status fixture and a journal fixture to try the GUI without a Comin host:

```console
printf '#!/bin/sh\ncat tests/fixtures/status_normal.json\n' > /tmp/comin && chmod +x /tmp/comin
printf '#!/bin/sh\ncat tests/fixtures/journal_boot_deploy.jsonl\n' > /tmp/journalctl && chmod +x /tmp/journalctl
COMIN_TRAY_COMIN=/tmp/comin COMIN_TRAY_JOURNALCTL=/tmp/journalctl cargo run -- gui deployments
```

## Development

```console
nix develop
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
nix flake check
nix build
```
