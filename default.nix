{ pkgs ? import (import ./nixpkgs.nix) { } }:
let
  runtimeLibraries = with pkgs; [
    libglvnd wayland libxkbcommon libx11 libxcursor libxi libxrandr
  ];
in
pkgs.rustPlatform.buildRustPackage {
  pname = "media-launcher";
  version = "0.1.0";
  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      pkgs.lib.cleanSourceFilter path type
      && !(builtins.elem (builtins.baseNameOf path) [ "target" "result" ]);
  };
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
  buildInputs = with pkgs; [ udev ] ++ runtimeLibraries;
  strictDeps = true;
  postInstall = ''
    install -Dm644 ${./packaging/media-launcher.desktop} \
      $out/share/applications/media-launcher.desktop
    wrapProgram $out/bin/media-launcher \
      --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibraries}
  '';
  meta = {
    description = "Minimal controller-operated filesystem video launcher";
    mainProgram = "media-launcher";
    platforms = pkgs.lib.platforms.linux;
    license = pkgs.lib.licenses.mit;
  };
}
