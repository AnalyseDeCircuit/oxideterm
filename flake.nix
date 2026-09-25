{
  description = "OxideTerm packages for Nix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
    in
    {
      legacyPackages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          oxideterm = pkgs.callPackage ./nix/package.nix { };
        }
      );

      packages = forAllSystems (
        system:
        let
          package = self.legacyPackages.${system}.oxideterm;
        in
        {
          default = package;
          oxideterm = package;
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.oxideterm;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/oxideterm-native";
          };
          cli = {
            type = "app";
            program = "${package}/bin/oxideterm";
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            name = "oxideterm-dev-shell";
            packages =
              with pkgs;
              [
                cargo
                clippy
                clang
                cmake
                just
                makeWrapper
                nasm
                perl
                pkg-config
                rustc
                rustfmt
                libclang
              ]
              ++ (with pkgs; [
                alsa-lib
                dbus
                fontconfig
                freetype
                gst_all_1.gst-plugins-base
                gst_all_1.gstreamer
                krb5
                libGL
                libunwind
                libx11
                libxcb
                libxcursor
                libxfixes
                libxinerama
                libxkbcommon
                libxrandr
                openssl
                udev
                vulkan-loader
                wayland
              ]);
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          };
        }
      );

      overlays.default = final: _prev: {
        oxideterm = final.callPackage ./nix/package.nix { };
      };

      nixosModules.oxideterm =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        import ./nix/module.nix {
          inherit config lib pkgs;
          defaultPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.oxideterm;
        };
      nixosModules.default = self.nixosModules.oxideterm;

      checks = forAllSystems (system: {
        oxideterm = self.packages.${system}.oxideterm;
      });
    };
}
