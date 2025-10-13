{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    inputs@{ self, nixpkgs, ... }:
    let
      system = "x86_64-linux";

      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          inputs.rust-overlay.overlays.default
        ];
      };

      rust-bin = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      libs =
        with pkgs;
        (pkgs.lib.strings.optionalString stdenv.isLinux [
          glib
          # openssl_3
          openssl
          vulkan-headers
          vulkan-loader
          wayland
          wayland-protocols
        ])
        ++ [ ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        name = "ltrait";

        shellHook = ''
          export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libs}:$LD_LIBRARY_PATH
        '';

        buildInputs =
          with pkgs;
          [
            rust-bin

            cargo-nextest
            (pkgs.rust-bin.stable.latest.default.override { extensions = [ "rust-src" ]; })
          ]
          ++ (pkgs.lib.strings.optionalString stdenv.isLinux [
            fontconfig
            glib
            libxkbcommon
            openssl_3
            pkg-config
            vulkan-tools
            wayland-scanner
            xorg.libxcb
          ]);
      };
    };
}
