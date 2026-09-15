# Fixed revision and compressed-archive checksum; no ambient channel dependency.
builtins.fetchTarball (builtins.fetchurl {
  url = "https://api.github.com/repos/NixOS/nixpkgs/tarball/c3eea5b2156db11c7eeeada3dc737711255b253e";
  sha256 = "6c932571e8f5da8e897da50c2dbb932bf760c8f51415187fb3db3c7e40cec41e";
})
