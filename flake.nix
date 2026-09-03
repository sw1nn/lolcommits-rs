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
          # OpenCV 4.10 predates ffmpeg 8, which dropped avcodec_close and
          # av_stream_get_side_data, so videoio fails to compile against it.
          # Camera capture goes through nokhwa, not OpenCV, so drop the backend.
          enableFfmpeg = false;
          # nixpkgs binds the contrib sources to a `let` variable, not a
          # derivation attribute, so overrideAttrs cannot re-pin them and they
          # stay on the nixpkgs version while `src` below drops to 4.10.0. The
          # crate only uses core, imgproc and dnn, none of which are contrib
          # modules, so build without contrib instead of fighting the mismatch.
          enableContrib = false;
        }).overrideAttrs (oldAttrs: rec {
          version = "4.10.0";
          src = pkgs.fetchFromGitHub {
            owner = "opencv";
            repo = "opencv";
            rev = version;
            sha256 = "sha256-s+KvBrV/BxrxEvPhHzWCVFQdUQwhUdRJyb0wcGDFpeo=";
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

          # The daemon reads its gallery assets from disk, defaulting to the
          # packaged /usr/share location. Point a development run at the tree.
          shellHook = ''
            export LOLCOMMITS_STATIC_ROOT="''${LOLCOMMITS_STATIC_ROOT:-$PWD/assets/static}"
          '';

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
          LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";
        };
      }
    );
}
