{
  description = "lolcommits development environment";

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

        # Pin OpenCV to version 4.10.0
        opencv410 = (pkgs.opencv.override {
          # Use protobuf 27 for compatibility with OpenCV 4.10.0
          protobuf = pkgs.protobuf_27;
        }).overrideAttrs (oldAttrs: rec {
          version = "4.10.0";
          src = pkgs.fetchFromGitHub {
            owner = "opencv";
            repo = "opencv";
            rev = version;
            sha256 = "sha256-s+KvBrV/BxrxEvPhHzWCVFQdUQwhUdRJyb0wcGDFpeo=";
          };
          contrib = pkgs.fetchFromGitHub {
            owner = "opencv";
            repo = "opencv_contrib";
            rev = version;
            sha256 = "sha256-JFSQQRvcZ+aiLUxXqfODaWQW635Xkkvh4xmkNcGySh8=";
          };

          # Patch OpenCV source to fix CMake 4.x compatibility
          postPatch = (oldAttrs.postPatch or "") + ''
            # Fix cmake_minimum_required in OpenCVGenPkgconfig.cmake for CMake 4.x
            substituteInPlace cmake/OpenCVGenPkgconfig.cmake \
              --replace-fail 'cmake_minimum_required(VERSION 2.8.12.2)' 'cmake_minimum_required(VERSION 3.5)'
          '';
        });
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            # Build tools and compilers
            pkg-config
            clang
            llvmPackages.llvm
            llvmPackages.libclang

            # Rust toolchain from rust-toolchain.toml
            (rustVersion.override { extensions = [ "rust-src" "llvm-tools-preview" ]; })
            cargo-llvm-cov
            rust-analyzer
          ];

          buildInputs = with pkgs; [
            # Runtime libraries
            opencv410
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
