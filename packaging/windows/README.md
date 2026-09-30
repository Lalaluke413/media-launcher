# Windows packaging (first attempt)

Run from a Windows checkout, using Windows PowerShell 5.1 or PowerShell 7.
Prerequisites:

- Rust 1.88+ with target `x86_64-pc-windows-msvc` (install with rustup if needed).
- Visual Studio Build Tools: desktop C++ workload and Windows SDK.
- 7-Zip (7z.exe on PATH or in its default Program Files location).
- Windows' `curl.exe` on PATH (included with current Windows 10/11).
- Inno Setup **6.3 or newer**, only if building the installer.

From the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\packaging\windows\build-bundle.ps1
```

Add `-Installer` to build the EXE installer as well:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\packaging\windows\build-bundle.ps1 -Installer
```

`-SevenZip 'C:\path\7z.exe'` and `-InnoSetup 'C:\path\ISCC.exe'` override tool
locations. Run as a normal user; neither packaging nor installation needs admin.
The execution-policy setting applies only to this PowerShell process.

The script builds the application with a static MSVC CRT, downloads the exact
assets in dependencies.json, verifies SHA-256 hashes (including cached assets),
and assembles a baseline x64 bundle with libmpv, yt-dlp, Deno and the Vulkan loader
required by this libmpv build. No system Vulkan installation is performed. It copies license
texts and a dependency manifest, then creates a ZIP. No downloads use `latest`.
Generated files go to `dist/windows/`; downloads are cached under
`target/packaging/downloads/`. Downloads use `curl.exe`, first write to a temporary
file, and enter the cache only after checksum verification. Invalid cached files
are downloaded again automatically. Earlier/partial bundles are moved to a
`.previous-<id>` folder when rebuilding; these can be removed when no longer needed.

## Please test on Windows now

1. Build the bundle, then run its media-launcher.exe with your existing config.
   Explicit `player.libmpv` or `yt_dlp.executable` values take precedence; remove
   those overrides to exercise automatic bundle discovery.
2. Try local video, a YouTube URL from the phone, and your custom extractor.
   Check pause, seek, Back and quitting. Run from a different working directory
   as well. No separate mpv, Python, yt-dlp or Deno installation should be needed.
3. If Inno Setup is available, build and run the installer. Its default path is
   `%LOCALAPPDATA%\Programs\Media Launcher`, with a Start Menu shortcut.
4. Add the installed executable to Steam. Reinstall over it and confirm config,
   plugins, and the executable path remain intact. A later higher-version build
   is still needed for a full upgrade test.

## Installation and updates

The installer uses a fixed AppId and remembers the previous installation folder.
Users manually run a newer installer to upgrade the app and bundled dependencies.
The ZIP uses normal per-user data, not a self-contained portable profile.
Configuration/plugins are not written into the installation folder or deleted by
the uninstaller. There is no automatic updater or update check in this pass.

Future managed yt-dlp updates are reserved for `<app data>/dependencies/yt-dlp/`.
That path is not created or searched yet; the installer never writes there.
Explicit executable overrides will stay user-managed.

This is an unsigned local-test bundle. Complete the third-party source/notice
work in THIRD-PARTY.md before public binary distribution. Windows native build,
installer and playback verification are pending your test; Linux checks cannot
substitute for them. The dependencies are pinned, but Rust/toolchain differences
mean this is not a promise of byte-for-byte identical output.
