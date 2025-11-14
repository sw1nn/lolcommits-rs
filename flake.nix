{
  description = "lolcommits-rs development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustc
            cargo
            rustfmt
            clippy

            # OpenCV dependencies
            opencv
            clang
            llvmPackages.libclang
            pkg-config

            # Additional libraries
            libgit2
            openssl
          ];

          shellHook = ''
            export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
            export LD_LIBRARY_PATH="${pkgs.opencv}/lib:${pkgs.libgit2}/lib:$LD_LIBRARY_PATH"
            export PKG_CONFIG_PATH="${pkgs.opencv}/lib/pkgconfig:$PKG_CONFIG_PATH"
          '';
        };
      }
    );
}
