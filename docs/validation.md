# Packaging first attempt — 2026-09-29

Windows follow-up: the user reported a successful MSVC release build, followed
by a libmpv checksum mismatch from the Invoke-WebRequest download. The file was
31,490,080 bytes with SHA-256 `dde9661bebb5a6042bfed26ff04d0c2bbf13cad95535b55a119c932fe0aa84f1`.
The user's fresh curl.exe download matched the pinned
`81795d759e01016f1550fd71651a1a5d59ab5c28ef31c0b6793224e9cff39459`;
a fresh Linux download and GitHub release metadata also matched that pin.
The bundle script now uses curl.exe, verifies temporary files before caching,
replaces invalid cache entries, and preserves earlier bundles when retrying.
Checksums remain unchanged. The user subsequently verified successful bundling,
bundled playback, installer compilation/installation, and launch from Steam.
The reason the first download's bytes differed has not been established.

The user reported a borderless 720p window at the screen's upper left despite
fullscreen defaults. Startup previously passed a windowed inner size and
`maximized = false` alongside fullscreen. egui-winit reapplies those after native
creation. Startup now supplies only the geometry appropriate to the chosen mode;
windowed startup requests centering, and leaving fullscreen restores configured
windowed size. Regression coverage checks these startup attributes. Current
checks: 19 unit tests passed, Linux Clippy clean, Windows cross-check clean.
The window fix awaits native Windows verification.

Windows packaging adds a PowerShell bundle builder, pinned SHA-256-verified
libmpv/yt-dlp/Deno/Vulkan-loader downloads, and an Inno Setup per-user installer.
The Arch VCS PKGBUILD declares system dependencies. No CI or updater was added.

## Lightweight checks passed

- Existing `cargo test --locked --offline`: 18 passed, two native tests ignored.
- Linux and Windows-target Clippy with warnings denied: clean.
- Windows GNU `cargo check --locked --offline --all-targets`: clean.
- Formatting and `git diff --check`: clean.
- `bash -n packaging/arch/PKGBUILD` and `makepkg --printsrcinfo`: passed.
- Downloaded libmpv and matching runtime archive hashes matched upstream GitHub
  release digests. DLL imports were inspected: this libmpv hard-imports
  `vulkan-1.dll`, which is not in either mpv archive. The script therefore bundles
  the x64 loader from the official LunarG components ZIP; its downloaded hash
  matched the pinned value and its x64 DLL/license paths were inspected.

## Ready for user testing

PowerShell and Inno Setup are not available here. The user verified the Windows
MSVC release build, bundle, installer, native playback, and Steam launch.
Reinstall/upgrade behavior and the latest window fix still need Windows testing.
Start with packaging/windows/README.md.

The Arch package definition has not been built, installed, or submitted to AUR.
It tracks committed upstream Git source; instructions explain how to test a
local source repository instead. Complete third-party notices/source materials
before publishing Windows binary releases (see THIRD-PARTY.md).

---

# Embedded playback validation — 2026-09-29

Stage 3 defaults to libmpv playback inside the existing egui/OpenGL window,
retains the external backend, and adds portable application-window settings.
The user reported Stage 2 working on both Windows and Linux before this change.

## Passed

- `cargo build --locked --offline`: Linux application built.
- `cargo test --locked --offline`: **18 passed**, 0 failed; two native/dependency
  acceptance tests are ignored by default.
- `cargo test --locked --offline --test embedded_playback -- --ignored --nocapture`:
  **1 passed**, running a real native OpenGL window with installed libmpv 0.41.0
  and yt-dlp on Linux. The test uses generated Y4M video and a localhost server,
  with all media, configuration, plugin, and watch-later fixtures in temporary
  directories. It verifies:
  - Video pixels have the expected colors and orientation after window resizing.
  - UI overlays remain visible after libmpv renders.
  - Pause holds playback position; seeking while paused preserves pause.
  - Back stops playback and honors mpv's existing resume preference.
  - A second playback starts unpaused; EOF ends playback cleanly.
  - Missing media reports an error and subsequent playback succeeds.
  - URL playback invokes a user yt-dlp extractor and renders the resulting video.
  - Normal mpv scripts and unrelated script options survive application overrides.
  - Conflicting `vo`, `keep-open`, and extractor-path settings in mpv.conf do not
    override application-owned playback integration.
  - Legacy mpv input.conf bindings do not compete with application controls.
  - Closing while video is playing frees render resources before player/window
    teardown and returns without hanging.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: clean.
- `cargo test --locked --offline mpv_loads_plugins -- --ignored --nocapture`:
  **1 passed**, confirming the retained external backend still resolves media
  through extractors in the user and extra plugin directories.
- `cargo check --locked --offline --target x86_64-pc-windows-gnu --all-targets`:
  application, library, and tests cross-compile for Windows without libmpv linker
  files. This does not establish Windows runtime behavior.
- `cargo fmt --check`, `git diff --check`, and updated CLI help: clean.

Configuration tests cover window modes, size validation, backend/library choices,
and CLI precedence. Input tests cover playback key edges and neutral gating.

## Still required

- Native Windows embedded playback using an API 2 libmpv DLL and its dependencies.
- Physical controller and Steam Big Picture playback/return/quit on both platforms.
- Linux Wayland, hardware decoding, subtitles/audio tracks, real audio output,
  browser-cookie extraction, real streaming services, and 4K/TV checks.
- Visual review of the application's basic playback overlay. The pixel acceptance
  test verifies rendering and layering in a dedicated test window, not UI polish.

Bundling dependencies, controller remapping, and richer player/settings menus
remain later stages. The external backend remains explicitly selectable.

---

# Configuration and customization validation — 2026-09-29

Stage 2 adds persistent TOML configuration, standard per-user configuration/data
locations, CLI overrides, an optional local library, and yt-dlp customization.
No CI was added.

## Passed

- `cargo build --locked --offline`: Linux application built.
- `cargo test --locked --offline`: **16 passed**, 0 failed; one dependency-based
  integration test is ignored by default.
- `cargo test --locked --offline mpv_loads_plugins -- --ignored --nocapture`:
  **1 passed** using installed mpv 0.41.0 and yt-dlp on Linux. A temporary
  localhost HTTP server supplies WAV audio. mpv invokes a custom yt-dlp
  executable and plays media resolved by extractors in both the app's user
  directory and an extra plugin directory. Executable/plugin paths include
  commas, Unicode, and apostrophes. This test needs network socket permission
  even though no external media service is contacted.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: clean.
- `cargo check --locked --offline --target x86_64-pc-windows-gnu --all-targets`:
  application and tests cross-compile for Windows; no Windows linking/runtime
  validation is implied.
- `cargo fmt --check` and `git diff --check`: clean.
- CLI smoke checks: `--help` and `--show-paths` do not create configuration/data
  directories; Linux XDG path overrides are respected; invalid configuration
  exits with code 2 before GUI/server startup.

Configuration tests cover first-run creation, preserving edits and extractor
files, missing explicit configuration, unknown keys/malformed TOML, relative
path bases, CLI precedence without writeback, repeatable plugin overrides,
invalid settings, and repeated yt-dlp arguments with special characters. The
browser also has a URL-only state test. The original Unix child-process test
still checks literal media arguments, single-player enforcement, and exit status.

## Still required

- Native Windows build/runtime checks, browser-cookie extraction, and playback
  using the eventual bundled yt-dlp distribution.
- Interactive inspection of the URL-only waiting screen and real phone access.
- Steam, physical-controller, Wayland/X11 window restoration, and couch/4K
  checks listed in the Stage 1 record below.

---

# Portable foundation validation — 2026-09-29

Stage 1 removes Nix packaging and machine-specific defaults. No CI was added.
Validation ran on Linux; Windows was checked using the GNU target from Linux.

## Passed

- `cargo build --locked --offline`: Linux executable built.
- `cargo test --locked --offline`: **10 passed**, 0 failed.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: clean.
- `cargo fmt --check` and `git diff --check`: clean.
- `target/debug/media-launcher --help`: reports current-directory and PATH defaults.
- `cargo check --locked --target x86_64-pc-windows-gnu --all-targets`:
  application and applicable test code compile for Windows. This checks code
  without linking or running a Windows executable. Locked Windows dependencies
  and the Rust Windows target were downloaded for this check.

General browsing, extension filtering, history, missing media, and input tests
are portable. Unix permission, non-UTF-8 filename, symlink, and shell-based
child-process integration tests remain enabled on Unix only. Discovery tests use
filenames that can coexist on case-insensitive Windows filesystems.

## Still required

- A native Windows build and test run, including the documented MSVC toolchain.
- Actual mpv playback on Windows, Windows paths and logging, and phone submission
  through the Windows firewall.
- Physical controller input and reconnection on both platforms.
- Steam Big Picture launch → local playback → return → URL playback → return → quit,
  using only the controller, on both platforms. Set an explicit library root and
  mpv executable in the shortcut.
- Linux Wayland and X11 fullscreen stacking and restoration after mpv exits.
- Held-button input gating after playback and real 4K couch readability.

Cross-compilation and automated tests do not establish these hardware behaviors.
mpv and yt-dlp still need separate installation/configuration; embedding and
bundled releases are later stages.

---

The following record describes the original implementation before portability
work and is retained for historical context.

## Original implementation validation — 2026-09-14

Environment: Arch Linux, Rust 1.97.1, X11 session reported by the environment.
Nix is not installed here. No system configuration was changed.

## Passed

- `cargo check`: eframe with explicit Wayland/X11/GL support and gilrs compile.
- `cargo build --locked --offline`: runnable `target/debug/media-launcher` built.
- `cargo test --locked --offline`: **8 passed**, 0 failed.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- `target/debug/media-launcher --help`: usage and defaults printed successfully.
- `target/debug/media-launcher --ui-scale NaN`: rejected with exit code 2.

The automated tests exercise all eight extensions; hidden entries; deterministic
sorting; symlinks inside/outside the root, broken and retargeted after listing;
non-UTF-8 paths; selection clamping, history, and refresh; unavailable roots,
removed files, permission denial; directional repeat and action edges; neutral
handoff gating; simulated disconnect snapshots; and real child processes with
literal unusual filename arguments, unsuccessful/successful exits, and one-child
enforcement. The fake-player shell and `true` are resolved through PATH so tests
can run in a Nix build sandbox without `/bin/sh` or `/bin/true`.

The pinned nixpkgs archive was downloaded and its SHA-256 recorded. Package names
and `buildRustPackage`'s `cargoLock` interface were inspected in that revision.
The Nix derivation excludes local Cargo build artifacts from its source.

## Not performed

- Nix evaluation, development-shell entry, or `nix-build` (Nix unavailable).
- Interactive GUI tests, actual 4K/60 Hz measurements, couch readability, or
  visual scrolling checks.
- Physical controller, hotplug, or mpv SDL input testing.
- Customized mpv playback/resume behavior on real media.
- Steam Big Picture tracking, Wayland stacking/focus restoration, or repeated
  controller-only playback handoff on the target compositor.

The input state-machine tests and fake-player tests do **not** establish those
hardware behaviors. Follow the target-machine checklist in the README before
considering the spec's hardware acceptance checks complete.
