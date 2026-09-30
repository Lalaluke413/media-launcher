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
