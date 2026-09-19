# Comin Tray

Comin Tray is a small native tray application for Comin on Plasma.
It uses `ksni` for the StatusNotifierItem and `zbus` for notifications.

The application reads the structured output from `comin status --json`.
It uses Comin commands for all actions.
It does not inspect Nix store paths or run Nix activation scripts.

## Features

- Shows idle, fetch, evaluation, build, deployment, failure, suspension, and restart states.
- Shows the selected Git source and the latest deployment.
- Fetches the configured remotes.
- Suspends or resumes Comin.
- Activates a successful boot deployment with a live switch when safe.
- Sends desktop notifications through D-Bus.

## Run

```console
nix run github:JuanDelPueblo/comin-tray
```

The package does not add an autostart entry.
Start it with your preferred Plasma autostart method.

## Design

Comin Tray targets the JSON contract from Comin `v0.14.0`.
The flake packages this Comin version by default.
A parent flake can make the `comin` input follow its own Comin input.

The tray polls status every three seconds.
This small design avoids generated gRPC code and keeps Comin as the state owner.
The live switch calls this Comin command:

```console
comin deployment submit-latest --operation switch
```

The action only appears for a successful latest deployment under a `boot` policy.

## Development

```console
nix develop
cargo test
nix flake check
```
