# Arch package (first attempt)

`media-launcher-git` builds the Git source and depends on system mpv (which
provides libmpv), yt-dlp, Deno, and native window/input libraries. No application
updater modifies these packages. Upgrade through your normal AUR workflow.
No other Linux distribution packaging is included in this pass.

## Test the current checkout

Install the standard Arch packaging tools (`base-devel`) and use a disposable
build directory. The upstream Git source will not contain uncommitted work;
override the source to point at this checkout for the first test:

```sh
mkdir -p /tmp/media-launcher-arch-build
cp packaging/arch/PKGBUILD /tmp/media-launcher-arch-build/
```

Edit the copied PKGBUILD's `source` line to:

```bash
source=('media-launcher::git+file:///absolute/path/to/media-launcher')
```

This still builds **committed** source. To include this pass before committing,
make a source snapshot with Git in a temporary folder, and use that folder's
file URL. Do not run makepkg as root. Then:

```sh
cd /tmp/media-launcher-arch-build
makepkg -si
```

`makepkg -s` asks pacman to install missing dependencies. `cargo fetch` runs in
prepare; build uses the lockfile and cached crates. No elaborate check() suite
is included in this first package. Inspect with namcap if installed.

After installation, run `media-launcher` or select its desktop entry. Existing
per-user config and plugins are used; the example is under
`/usr/share/doc/media-launcher-git/`. Uninstallation leaves user data intact.

## Publication

This definition has not been submitted to AUR. After local verification, generate
`.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO` and publish the PKGBUILD and
.SRCINFO to the separate AUR repository. Keep the definition here in sync.
The VCS package version is derived from Cargo's version, commit count and hash.
