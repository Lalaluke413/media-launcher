# media-launcher

A fullscreen folder browser for a controller and your existing mpv. No database,
indexing, metadata, or network access. Lists immediate folders and videos, with
folders first. Hidden names, broken symlinks, and symlinks outside the library
root are omitted. Supported extensions (case-insensitive): `mp4 mkv m4v webm avi
mov ts m2ts`. Filenames and native filesystem paths are retained independently.

## Build and run

```sh
nix-build
./result/bin/media-launcher

# Development (uses the same pinned nixpkgs):
nix-shell
cargo run --locked -- --root /srv/downloads/complete
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

`nixpkgs.nix` pins both the revision and archive checksum. `Cargo.lock` pins Rust
packages. Nix is not needed on other Linux distributions: install Rust >= 1.88,
a C linker, pkg-config, libudev development files, Wayland, libxkbcommon, OpenGL/
EGL, and X11 libraries, then run `cargo build --locked --release`.

```sh
media-launcher --root /srv/downloads/complete \
  --mpv /run/current-system/sw/bin/mpv --ui-scale 1.5
```

Defaults: root `/srv/downloads/complete`, mpv
`/run/current-system/sw/bin/mpv`, UI scale `1.0`. Scale accepts `0.5`–`4.0` and
multiplies the desktop's display scale. Body text is 30 logical pixels; try `1.5`
or `2` on a 4K TV with desktop scaling disabled. Relative roots are accepted.
`--mpv` accepts an executable path or a command found in PATH. No mpv package is
installed or substituted; its existing user configuration and resume behavior
remain in charge. Run `media-launcher --help` for syntax.

## NixOS installation

Keep this project at a stable path, then add to your existing configuration:

```nix
{ ... }: {
  environment.systemPackages = [
    (import /path/to/media-launcher/default.nix { })
  ];
}
```

This uses the project's pinned package set even if your system uses another
revision. To intentionally build against your system's package set instead, use
`import /path/to/media-launcher/default.nix { inherit pkgs; }` in a module with a
`pkgs` argument. That overrides the project's nixpkgs pin.

Apply your configuration using your normal NixOS workflow. The installed
executable is `/run/current-system/sw/bin/media-launcher`; the desktop entry is
**Videos**. Keep your customized mpv installed at its existing system path.
Existing Wayland, NVIDIA drivers, Steam, and filesystem permissions are used
as-is. Nothing in this repository modifies system configuration or permissions.

## Steam Big Picture

1. In desktop Steam choose **Games → Add a Non-Steam Game to My Library**.
2. Choose **Videos**, or browse to `/run/current-system/sw/bin/media-launcher`.
3. Open shortcut **Properties → Shortcut**. Set:
   - Name: `Videos`
   - Target: `/run/current-system/sw/bin/media-launcher`
   - Start In: `/srv/downloads/complete`
   - Launch Options: `--root /srv/downloads/complete --mpv /run/current-system/sw/bin/mpv --ui-scale 1.5`
4. Under **Properties → Controller**, set the override to **Disable Steam Input**.
5. Launch **Videos** in Big Picture. Leave compatibility/Proton disabled.

Adjust only the scale and paths as needed. The launcher stays alive throughout
playback so Steam tracks the whole session.

## Controls

| Controller (physical position) | Keyboard | Action |
| --- | --- | --- |
| D-pad / left stick up/down | Up / Down | Move, clamped at ends |
| A / south | Enter | Open folder / play; retry error |
| B / east | Escape | Parent folder / dismiss error |
| X / west | R | Refresh / retry |
| Start / menu | Q | Quit |

The stick dead zone is 0.35; repeat starts after 350 ms and repeats every 100 ms.
Controllers can disconnect and reconnect. Selection and scroll are remembered
per directory for this session. Refresh preserves the selected path, or falls
back to the nearest valid row. Back at root does nothing. Selected names wrap
in the footer (scrollable for exceptionally long names).

During playback **all launcher actions, including window-close requests, are
disabled**. gilrs reads nonexclusively and continues draining events; it does not
grab the controller. mpv receives the physical controller through its existing
SDL bindings. After playback, controls must be neutral for 200 ms before new
presses are accepted. This also applies at launcher startup. No focus requests,
minimize/unminimize commands, or compositor-specific graphics workarounds are
issued: the fullscreen browser remains underneath mpv.

The UI sleeps when idle; a small input thread polls for gamepads every 8 ms and
wakes the UI on state changes. Holding a direction requests 60 Hz updates.
Playback is checked every 100 ms without blocking the UI.

## Errors and diagnostics

Missing roots, unreadable directories, removed files, failed spawns, and failed
player exits leave the browser usable. Retry with X, dismiss with B, then B again
to go back. Directory-entry errors are logged and skipped. The application never
changes ownership or access permissions.

Each playback writes stdout/stderr to a private regular file named
`$TMPDIR/media-launcher-<pid>-<timestamp>.log` (normally `/tmp`). Its path is printed
to the launcher's stderr and shown on unsuccessful exit. Output is not piped, so
an undrained pipe cannot stall mpv. These temporary diagnostics are not watched
state; remove old logs when no longer needed. Launcher diagnostics go to stderr.

## Validation status

Automated checks cover filtering and sorting, native/non-UTF-8 and unusual paths,
symlink confinement, directory history, selection recovery, missing roots and
removed files, directional repeat, one-shot actions, playback neutral gating,
argument-vector launching, single-child enforcement, and failed/successful
player exits. See [validation notes](docs/validation.md) for actual results.

**Target-machine acceptance is still required.** This project was implemented
on Arch/X11 without Nix, a test controller, or the target Steam/Wayland setup.
In particular, fullscreen stacking and restoration depend on the compositor;
they are not claimed to be validated here. On your NixOS machine:

- Record `nixos-version`, `$XDG_CURRENT_DESKTOP`, `$XDG_SESSION_TYPE`, the controller
  model, and the customized mpv version.
- Build with `nix-build`, launch through Steam, and use only the controller for
  folder → video → return → second video → return → quit, several times.
- Hold B/stick while exiting mpv: the browser must stay at its original location
  until neutral and a new press. Test Start during playback too.
- Check 4K couch readability, long-name scrolling, empty/unreadable folders,
  refresh after adding/removing a file, and controller reconnection.
- Check mpv opens above the browser and closing it restores a usable browser.
  If your compositor behaves differently, record that behavior before choosing
  a compositor-specific window rule or another handoff strategy.

Dependency references: [eframe 0.33.2 Wayland/GL features](https://docs.rs/crate/eframe/0.33.2/features),
[gilrs input and hotplug API](https://docs.rs/gilrs/0.11.2/gilrs/struct.Gilrs.html).
