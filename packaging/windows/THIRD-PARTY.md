# Bundled components

Exact download URLs and SHA-256 hashes are in dependencies.json.

- libmpv: shinchiro/mpv-winbuild-cmake, baseline x86_64 build 20260928,
  mpv revision e470f8986e. GPL build; GPL text is in licenses/mpv-LICENSE.txt.
  Source: https://github.com/mpv-player/mpv/tree/e470f8986e
  Build project: https://github.com/shinchiro/mpv-winbuild-cmake
  Binary release: https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20260928
  This DLL contains third-party codec/native libraries.
- yt-dlp: official standalone release 2026.08.19. License text is in
  licenses/ytdlp-LICENSE.txt. The executable includes Python and other components:
  https://github.com/yt-dlp/yt-dlp/tree/2026.08.19
  https://github.com/yt-dlp/yt-dlp/blob/2026.08.19/README.md#dependencies
- Deno: official x64 release v2.9.7, MIT. License text is in
  licenses/deno-LICENSE.txt. Source: https://github.com/denoland/deno/tree/v2.9.7
- Vulkan loader: official LunarG runtime components 1.4.328.1. Only the x64
  loader DLL is included, beside the application; no runtime installer is run.
  VulkanRT-License.txt is copied from the archive, including its component terms.
  Source: https://github.com/KhronosGroup/Vulkan-Loader/tree/vulkan-sdk-1.4.328.1

These are initial local testing artifacts. Before publishing binaries, assemble
complete third-party notices and corresponding source/build materials for the
native GPL bundle and embedded dependencies. The application's MIT source
license does not replace the bundled components' licenses.
