# Backup Verify

A Windows/macOS desktop app that watches your backup folders and tells you
— quietly, only when something's actually wrong — whether they're still
there, still updating, and still intact. Not "a backup exists", but "this
backup would actually save you."

Point it at a folder (an external drive, a NAS share, a synced cloud
folder), pick how often to check, and it does the rest in the background:
reachability, freshness, and sampled corruption checks. See
`docs/architecture.md` for what "verification" means for a plain folder
and why.

## Status

MVP: folder-based backups only (no restic/borg repo support yet — see
`docs/architecture.md`), offline license-key gating, not yet code-signed
or publicly distributed.

## Developing

```
npm install
npm run tauri dev
```

Requires the Rust toolchain and Tauri's platform prerequisites
(https://tauri.app/start/prerequisites/).

## Building

```
npm run tauri build
```

Cross-platform release builds run in CI on tagged pushes — see
`.github/workflows/release.yml`. There is no reliable way to build a real
macOS `.dmg` from Linux, so local development on Linux is fine but the
actual Windows/macOS installers come from CI or a machine running that OS.
