# modsync

One-click mod-profile sync between friends for [r2modman](https://github.com/ebkr/r2modmanPlus) / Thunderstore-based games. Add a friend by a short share code, hit sync, and your mod list matches theirs — no more manually comparing lists or re-downloading mods one by one before a co-op session.

Built for games like R.E.P.O. and PEAK, and anything else r2modman manages.

## How it works

1. Open modsync — it finds your existing r2modman profiles automatically.
2. Pick a profile and click **Share this profile**. You get a short share code.
3. Send that code to a friend; they add you as a friend using it (and you add theirs).
4. Either of you can preview what a sync would change (installs, updates, enables/disables) before running it.
5. Hit **Sync now** — modsync downloads/installs whatever's missing, flips enable state to match, and leaves everything else alone.

There are no user accounts. Pairing is just a share code backed by an owner secret only your own client ever sees — a [Supabase](https://supabase.com) project stores each shared profile's mod list so friends can pull it, nothing more.

## Installing

Grab the latest installer for your platform from the [Releases page](https://github.com/null00void/modsync/releases):

- **Windows:** `modsync_x64-setup.exe` (or the `.msi`, if you prefer)
- **Linux:** `modsync_amd64.AppImage` — download, `chmod +x`, run

modsync checks for updates on launch and can update itself in place.

macOS isn't currently supported — open an issue if you'd like it.

## Development

Requires [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev    # run in dev mode
npm run tauri build  # produce a release bundle for your OS
```

Backend tests: `cd src-tauri && cargo test`.

CI (`.github/workflows/`) builds and tests on both Windows and Linux on every push, and publishes signed release artifacts (installers + the updater manifest) when a `vX.Y.Z` tag is pushed.

## Tech

Tauri (Rust) backend, SvelteKit frontend, Supabase (Postgres) for the sync backend. See `src-tauri/src/core/` for the interesting parts: `installer.rs` (replicates r2modman's own Thunderstore package install rules), `mods_yml.rs`, `diff.rs`, and `lock.rs`.

## License

MIT
