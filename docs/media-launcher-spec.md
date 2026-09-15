# Minimal controller-operated video launcher — v0.1

## Goal

Build a small fullscreen Linux application that displays folders and video files, accepts native controller input, and launches the selected file in the user's existing mpv installation. The filesystem is the library. Keep the implementation simple and easy to maintain.

## Target setup

- OS: NixOS, running an existing Wayland desktop session. The compositor and NixOS/nixpkgs revision are unspecified; inspect them when implementing on the target machine.
- Display: TV at 3840 × 2160, 60 Hz. The UI must be comfortably readable from a couch and navigate smoothly at 60 fps; continuous rendering while idle is unnecessary.
- GPU: NVIDIA RTX 2060. Playback currently works. A previous NVIDIA kernel/userspace driver mismatch was resolved by rebooting; do not carry forward graphics workarounds from that incident.
- Entry point: a non-Steam application shortcut in Steam Big Picture. Steam Input must be disabled for this shortcut. Also support launching directly from the desktop or terminal.
- Input: physical gamepad, exact model unspecified. Both the launcher and mpv must use native controller input; no Steam keyboard emulation is required.
- Library root: `/srv/downloads/complete`.
- Player: existing customized mpv with SDL2 gamepad support and working playback/controller configuration. Expected executable: `/run/current-system/sw/bin/mpv`. Preserve its normal user configuration, controller bindings, and resume behavior.
- Files may be owned by `qbittorrent:downloads`; use the logged-in user's existing filesystem access. Do not change ownership or permissions automatically.

This replaces Pegasus for video browsing. Pegasus omitted some matching MP4 files unless they were explicitly listed; this application's discovery must be transparent and based only on directory entries and extensions.

## Required behavior

1. Open fullscreen at the configured library root.
2. List immediate child directories and supported video files. Entering a folder lists its children; do not recursively index or flatten the library.
3. Include extensions `mp4`, `mkv`, `m4v`, `webm`, `avi`, `mov`, `ts`, and `m2ts`, matched case-insensitively. Do not probe codecs, interpret media titles, merge files, or require metadata. Each matching file gets its own entry.
4. Hide dotfiles and dot-directories. Sort directories first, then files, with deterministic case-insensitive name sorting and an exact-name tie-breaker. Preserve actual filenames and paths.
5. Constrain browsing to the configured root. Back at the root does nothing. For symlinks, permit targets within the canonical root only; skip broken links and outside-root targets. This is a navigation boundary, not a security sandbox.
6. Refresh on directory entry, return from playback, and an explicit refresh action. Preserve selection by path where possible; otherwise select the nearest valid row.
7. Remember each visited directory's selection and scroll position during the current session. Persistence across application restarts is optional, outside the MVP requirement.
8. Display clear empty-directory, unavailable-root, permission, and launch-error states. Allow retry or back/quit using the controller. One unreadable entry must not crash the application.

## Interface and input

Use a large, high-contrast scrollable list with a clearly highlighted selection, a current-path breadcrumb, folder/file indicators, and a short control legend. Keep the selected row visible. Long filenames must not break layout; expose the selected filename in full through wrapping or a separate detail line. Support display scaling and a simple UI-scale override.

Face-button labels below use Xbox-style names; map by physical position for other controllers.

| Control | Launcher action |
| --- | --- |
| D-pad up/down or left stick up/down | Previous/next row |
| A / south face button | Open folder or play file |
| B / east face button | Parent folder; dismiss error |
| X / west face button | Refresh directory |
| Start / menu | Quit launcher and return to Steam |
| Arrow keys, Enter, Escape, R, Q | Equivalent keyboard fallback |

Use an analog dead zone and predictable directional repeat (initial defaults: 350 ms delay, 100 ms repeat). Confirm, back, refresh, and quit trigger once per press. Clamp selection at list boundaries. Handle controller disconnect/reconnect without restarting or losing selection. Routine use must require no mouse or keyboard.

## mpv lifecycle and controller handoff

- Launch one mpv child at a time using an argument-vector process API, never a shell command string. Supply fullscreen and `--` before the absolute media path. Preserve spaces, Unicode, quotes, and other filename characters; retain native filesystem paths separately from display strings.
- Keep the launcher process alive until playback ends so Steam continues tracking the session. Monitor the child without blocking the UI event loop.
- While playback is active, disable all launcher navigation and actions, including quit. Do not steal focus or act on gamepad events intended for mpv. Release any exclusive controller ownership if applicable.
- Keep the launcher behind mpv, or use another tested approach that reliably returns to the browser afterward. Do not assume a Wayland client can force focus. Validate the window/focus behavior on the actual compositor and through Steam.
- On mpv exit, restore the browser with the same directory, selection, and scroll position. Discard stale input and require held buttons/sticks to return to neutral before accepting new launcher actions. The B press that closes mpv must not also navigate backward in the browser.
- Existing mpv bindings own play/pause, seeking, volume, subtitles, audio tracks, exit, and resume state. Do not rebuild these controls in the launcher or embed libmpv.
- A missing executable, failed spawn, or unsuccessful player exit must return to a usable browser with a concise error. Keep useful diagnostics in logs without letting child output block playback.

## Implementation and packaging

Prefer Rust. The previously proposed starting point is eframe/egui for the UI and gilrs for gamepads; SDL is an acceptable alternative if it simplifies reliable controller handoff. These are implementation options, not fixed dependency-version requirements. Verify Wayland support and Nix dependencies against the chosen versions.

Provide a small runnable project, pinned dependency lockfiles, and reproducible Nix build/development setup. Expose at least `--root`, `--mpv`, and `--ui-scale`; defaults must work with the setup above. Package a stable executable named `media-launcher` and a desktop entry. Do not silently substitute a stock mpv build lacking the user's gamepad support.

Include a short README with build/run commands, NixOS installation configuration, and the exact Steam non-Steam shortcut setup (installed target, launch options, Steam Input disabled). Steam and the Wayland session already exist; setting up desktop autologin or Steam autostart is outside this task. Do not modify the user's system configuration automatically.

## Out of scope

No media database, TMDB or other network services, posters, thumbnails, filename-based TV/movie identification, search, recursive indexing, playlists, file operations, custom playback engine, or launcher-owned watched/resume state. Do not impose an arbitrary source-line budget at the expense of correct input and process handling.

## Acceptance checks

- On the target NixOS Wayland desktop, launch from Steam Big Picture and complete folder navigation → playback → return → another playback → quit using only the controller.
- At 4K/60 Hz, text is readable from the couch, selection scrolls correctly, and navigation is responsive.
- Every supported visible file appears independently, including uppercase extensions and names containing spaces, Unicode, quotes, and shell metacharacters. No content probing or per-file exceptions are needed.
- Nested navigation stays within the root. Empty folders, missing paths, denied access, broken links, and files removed between listing and launch do not crash the app.
- During playback, the launcher does not respond to mpv controller actions. Closing mpv restores the original selection without an extra back/confirm action. Repeat this several times to check focus and input handoff.
- Controller reconnection works. Missing mpv and unsuccessful playback leave the browser usable. Refresh discovers newly added files.
- Add focused automated checks for filtering/sorting, path handling, and input/lifecycle state transitions where practical. Report build/test results and explicitly distinguish desktop/Steam/controller checks performed on real hardware from checks that remain unverified.
