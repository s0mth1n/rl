{
  description = "zl - a small Wayland app launcher, emoji picker and clipboard history, drawn on the CPU";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "zl";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.libxkbcommon ];
          meta = with pkgs.lib; {
            description = "A small Wayland app launcher, emoji picker and clipboard history, drawn on the CPU";
            license = licenses.mit;
            platforms = platforms.linux;
            mainProgram = "zl";
          };
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo rustc clippy rustfmt pkg-config libxkbcommon
            fontconfig wl-clipboard sway grim
          ];
        };
      });
    };
}
