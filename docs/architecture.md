# Architecture

## Why a desktop app, not a server

The original idea (see the product's own idea-rating writeup) was a Linux
server-side agent watching restic/borg repos. This version is deliberately
different: a Windows/macOS desktop app that watches plain backup **folders**
(an external drive, a NAS share, a synced cloud folder). That removes both
structural risks a server-side version would carry — the app never holds a
customer's backup-decryption secret, and it never pulls someone else's
backup data over the network. Everything happens on the user's own machine.

## What "verification" means for a plain folder

There is nothing to *restore* — the files are already sitting where they
are. So "verification" is three separate, weaker-but-still-useful signals,
computed in `src-tauri/src/checker.rs`:

1. **Reachability** — does the path still exist and resolve to a directory?
   Catches an unplugged drive or a renamed folder immediately.
2. **Freshness** — is the newest file's mtime within roughly 2× the backup's
   own schedule interval? Catches a backup job that silently stopped
   running, which is the single most common real-world backup failure.
3. **Sampled integrity** — a subset of previously-seen files gets re-hashed
   (BLAKE3) each run and compared against the stored baseline. If a file's
   mtime is unchanged but its hash differs, that's the signature of silent
   corruption (bit rot, a failing drive, a bad sync) rather than a
   legitimate edit, and it's flagged once.

## Sampling strategy

Hashing every file on every run doesn't scale to a large backup folder.
Two rules keep the cost bounded while still guaranteeing every file
eventually gets re-verified:

- **New files** (never seen before) are hashed immediately, up to
  `NEW_FILE_HASH_CAP` (3000) per run, to establish a baseline.
- **Known files** are sampled by *least-recently-verified* order (not
  randomly) — `db::least_recently_seen_paths` — so coverage rotates through
  the whole tree over successive runs instead of leaving some files
  permanently unchecked. Sample size is `max(25, 10% of known files)`,
  capped at 1000 per run.

This is tuned for a personal backup folder (thousands to a few hundred
thousand files), not a fileserver with millions of objects. If that ever
becomes a real use case, the constants at the top of `checker.rs` are the
place to revisit, not the algorithm shape.

## Two check engines, one MVP scope

`checker.rs` implements the folder engine described above. A structured
repository (restic, borg, a Time Machine snapshot) supports a *stronger*
check — an actual restore into a temp directory, then integrity check on
the restored data — but that's explicitly out of the MVP (see
`docs/distribution.md` for sequencing). The two are different enough
internally that they'll live as separate modules (`checker.rs` vs a future
`repo.rs`) dispatched by detecting the path's structure, not one unified
algorithm.

## Background scheduling

`scheduler.rs` runs a coarse 15-minute poll loop (`POLL_INTERVAL`) checking
which backups are due against their own schedule (daily/weekly/monthly).
Actual checks run via `spawn_blocking` so file hashing never stalls the
async runtime the UI and tray icon depend on. Closing the window hides it
(`lib.rs`'s `on_window_event`) rather than quitting — the point of the app
is the background schedule, not the window.

## Data storage

A single local SQLite database (`rusqlite`, bundled — no system SQLite
dependency) under the OS's app-data directory. Three tables: `backups`
(one row per tracked path), `checks` (append-only history), `file_hashes`
(the rolling baseline used for sampling). Nothing leaves the machine; there
is no server component in this MVP.
