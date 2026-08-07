# Auto-update

PiBeat checks GitHub Releases for a newer version, downloads it quietly in the
background, and shows a bar at the top of the window once the installer is
ready. Ported from AURA (`apps/desktop/updater.cjs` + `main.cjs`), which uses
the same flow.

## What the user sees

Nothing, until there is genuinely something to do:

1. Thirty seconds after launch, and every four hours after that, PiBeat asks
   GitHub for the latest release. Silently — no spinner, no dialog.
2. If a newer release exists **and** there is an installer for this platform,
   it downloads in the background.
3. Only once the installer is on disk does a bar appear:

   > ▲ PiBeat 0.3.0 is ready to install. **[Restart & install]** [Later] [Skip this version]

4. **Restart & install** takes a few seconds: PiBeat quits, the installer runs
   silently, and the new build starts.

The order matters. The obvious design — a modal the moment an update exists,
which *then* starts downloading — interrupts you first and makes you wait for
something you already agreed to. Downloading first means the interruption comes
once, at a point where acting on it is instant.

**Later** hides the bar; the next check brings it back. **Skip this version**
writes the tag to `update-skip.json` in the app config directory and that
version is never offered again.

## Manual check

**About → Check for updates** (click the PiBeat logo). Unlike the background
check, this always says something:

| Outcome | Message |
|---|---|
| Newer release, downloaded | "PiBeat 0.3.0 is downloaded and ready — see the bar at the top." |
| Newer release, no installer for this platform | "… Automatic install is Windows-only; use the releases page." + an **Open releases page** button |
| Up to date | "You're on the latest version (0.2.1)." |
| Releases not readable | "Cannot read the releases — the repository may be private. Set `GITHUB_TOKEN` to check." |
| Network / API failure | "Could not check for updates: `<reason>`" |

That last pair is the reason `check_for_update` returns a *status* rather than
an optional update. AURA spent a while unsure whether its update checking
worked at all, because a private repo answers 404 to an anonymous caller and
"no update" looked identical to "broken".

## Platform support

| Platform | Behaviour |
|---|---|
| Windows | Full: downloads the NSIS `-setup.exe`, installs silently with `/S`, relaunches. |
| macOS | Check only. A `.dmg` has to be mounted and dragged by hand, so the banner never appears; the About dialog offers the releases page. |
| Linux | Check only, for the same reason (`.AppImage` / `.deb`). |

`pick_asset` returns `None` off Windows deliberately — staging an installer the
app cannot run would produce a banner whose button does nothing.

## How the Windows install works

`install_update` writes a small `.cmd` and spawns it detached, then quits. The
script owns the sequence because each step needs the previous one finished:

```
wait for PiBeat to exit   (the installer cannot replace files that are in use)
run the installer /S
wait
start the new PiBeat
```

It logs each step to `update-install.log` in the app log directory, so a failed
update can be read back instead of guessed at.

Two details in that script are load-bearing, both learned in AURA:

- **`call`**, not a bare invocation and not `start /wait`. A bare call to a
  script target hands over control for good — PiBeat would install and never
  come back. `start /wait` opens a console window and blocks.
- **Brackets around `%errorlevel%`.** Without them `cmd` reads a leading `0` as
  a stream number and redirects instead of echoing, and the line vanishes from
  the log.

Spawning the installer and quitting *does* install — it just never comes back,
which from the user's side is indistinguishable from "nothing happened".

## Configuration

- **`GITHUB_TOKEN`** — optional, and only needed if the repository is ever made
  private. Unauthenticated calls to a private repo's releases return 404.
- Background checks are **disabled in debug builds**. A dev build is by
  definition not the released version, so it would offer an "update" every run.

## Why not `tauri-plugin-updater`?

Tauri's official updater is the better long-term home: it verifies signatures
and handles install on all three platforms. It needs a signing keypair, a
published `latest.json`, and `createUpdaterArtifacts` in the bundle config —
none of which exist yet. (`release.yml` does already pass
`TAURI_SIGNING_PRIVATE_KEY`, but that is boilerplate from the `tauri-action`
template; no key is configured and `tauri.conf.json` has no `plugins.updater`
section.)

Reading the Releases API works with the release pipeline exactly as it stands.
If the signing infrastructure is set up later, swapping the Rust side for the
plugin is contained — `UpdateBanner` and the About dialog talk to four commands
(`check_for_update`, `get_staged_update`, `install_update`, `dismiss_update`)
and would not need to change.

## Where the code is

| | |
|---|---|
| `src-tauri/src/updater.rs` | Pure logic: version comparison, asset selection, the Releases API call, download. HTTP is injected so all 12 tests run offline. |
| `src-tauri/src/lib.rs` (auto-update section) | Commands, background thread, staging, skip persistence, the Windows install script. |
| `src/components/UpdateBanner.tsx` | The bar. 9 tests in `UpdateBanner.test.tsx`. |
| `src/App.tsx` (`AboutModal`) | Manual check and its messages. |

## Testing it

```bash
cargo test --manifest-path src-tauri/Cargo.toml updater   # 12 tests, no network
npm test                                                  # 9 component tests
```

Neither suite touches the network or GitHub. What they cannot cover is the
Windows install itself — spawning a real installer and relaunching needs a real
Windows machine and a real signed release.
