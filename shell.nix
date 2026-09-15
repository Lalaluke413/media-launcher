let
  pkgs = import (import ./nixpkgs.nix) { };
  launcher = import ./default.nix { inherit pkgs; };
in pkgs.mkShell {
  inputsFrom = [ launcher ];
  packages = with pkgs; [ cargo rustc rustfmt clippy ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath launcher.buildInputs;
}
