{
  description = "zl - a small Wayland app launcher, emoji picker and clipboard history, drawn on the CPU";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      modules = import ./nix/module.nix { inherit self; };
    in
    {
      packages = forAll (pkgs: rec {
        zl = pkgs.callPackage ./nix/package.nix { };
        default = zl;
      });

      overlays.default = final: _prev: {
        zl = final.callPackage ./nix/package.nix { };
      };

      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = nixpkgs.lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        };
      });

      nixosModules.default = modules.nixos;
      homeManagerModules.default = modules.homeManager;

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            wl-clipboard
            sway
            grim
          ];
        };
      });

      checks = forAll (pkgs: {
        build = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });

      formatter = forAll (pkgs: pkgs.nixfmt-rfc-style);
    };
}
