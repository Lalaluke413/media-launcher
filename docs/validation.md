# Implementation validation — 2026-09-14

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
