{
  description = "lolcommits-rs development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustVersion = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            pkg-config
            clang
            llvmPackages.llvm
          ];

          buildInputs = with pkgs; [
            # Rust toolchain from rust-toolchain.toml
            (rustVersion.override { extensions = [ "rust-src" "llvm-tools-preview" ]; })
            cargo-llvm-cov
            rust-analyzer

            # OpenCV with clang-runtime feature
            opencv
            llvmPackages.libclang

            # Additional libraries
            libgit2
            openssl
            fontconfig.dev
          ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
          LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";
        };
      }
    );
}
