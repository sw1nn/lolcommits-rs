# Changelog
All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## v1.3.0 - 2025-11-18
#### Miscellaneous Chores
- (**pkg**) use sudo for sw1nn-makepkg-chroot - (66291d7) - Neale Swinnerton

- - -

## v1.2.0 - 2025-11-18
#### Features
- (**pkg**) use sw1nn-makepkg-chroot - (2ec6450) - Neale Swinnerton

- - -

## v1.1.1 - 2025-11-18
#### Tests
- isolate XDG directory tests with temp-env - (e188515) - Neale Swinnerton
#### Refactoring
- (**test**) eliminate directory-changing in git tests - (5b0aed5) - Neale Swinnerton

- - -

## v1.1.0 - 2025-11-18
#### Features
- (**build**) add SSH agent forwarding to chroot builds - (ec4a93f) - Neale Swinnerton
#### Refactoring
- (**git**) use revparse for SHA resolution - (fae513f) - Neale Swinnerton
#### Miscellaneous Chores
- (**pkg**) ignore build .log file - (7bf621b) - Neale Swinnerton

- - -

## v1.0.0 - 2025-11-18
#### Refactoring
- <span style="background-color: #d73a49; color: white; padding: 2px 6px; border-radius: 3px; font-weight: bold; font-size: 0.85em;">BREAKING</span>(**cli**) accept only SHA, retrieve message from git - (25cdd2f) - Neale Swinnerton

- - -

## v0.7.1 - 2025-11-17
#### Bug Fixes
- (**pkg**) rename build-chroot.sh -> build_chroot.sh - (faac2f5) - Neale Swinnerton

- - -

## v0.7.0 - 2025-11-17
#### Miscellaneous Chores
- (**pkg**) Get the build working - (2ba41c0) - Neale Swinnerton

- - -

## v0.6.3 - 2025-11-17
#### Bug Fixes
- (**pkg**) add llvm to PKGBUILD - (165eef5) - Neale Swinnerton

- - -

## v0.6.2 - 2025-11-17
#### Bug Fixes
- (**pkg**) add llvm-libs to PKGBUILD - (882e853) - Neale Swinnerton

- - -

## v0.6.1 - 2025-11-17
#### Bug Fixes
- correct package for AlgorithmHint - (d4d9f11) - Neale Swinnerton

- - -

## v0.6.0 - 2025-11-16
#### Features
- (**build**) add automatic OpenCV version detection for cvt_color API - (e4bcbdf) - Neale Swinnerton
- (**logging**) add journald support for systemd service deployment - (e055e76) - Neale Swinnerton
#### Miscellaneous Chores
- (**packaging**) add build-chroot.sh for clean Arch Linux builds - (dc31ce0) - Neale Swinnerton

- - -

## v0.5.0 - 2025-11-16
#### Features
- (**config**) add configurable bind address and port for server - (7da829b) - Neale Swinnerton

- - -

## v0.4.0 - 2025-11-16
#### Features
- (**config**) implement hierarchical config file search - (6be855a) - Neale Swinnerton
- (**config**) add --config flag to specify custom config file - (6d01fbd) - Neale Swinnerton

- - -

## v0.3.0 - 2025-11-16
#### Build system
- (**nix**) pin OpenCV to 4.10.0 with compatibility fixes - (59eac8b) - Neale Swinnerton

- - -

## v0.2.1 - 2025-11-15
#### Bug Fixes
- (**packaging**) disable LTO to resolve OpenCV linking errors - (51b7f63) - Neale Swinnerton
#### Tests
- (**git**) create temporary git repo fixture for tests - (902c595) - Neale Swinnerton
#### Build system
- (**packaging**) add .gitignore for Arch Linux build artifacts - (63ffdfc) - Neale Swinnerton

- - -

## v0.2.0 - 2025-11-15
#### Features
- (**config**) add configurable directories for images and models - (c9ce400) - Neale Swinnerton
- (**gallery**) add non-disruptive new image notifications - (ecbf81a) - Neale Swinnerton
- (**server**) add SSE auto-refresh for gallery and fix display issues - (5f2eaa9) - Neale Swinnerton
#### Refactoring
- (**paths**) use AsRef<Path> for path function parameters - (2effa03) - Neale Swinnerton

- - -

## v0.1.0 - 2025-11-15
#### Features
- (**background**) Replace OpenCV compositing with image crate - (d8e4dc4) - Neale Swinnerton
- (**background**) Add ML-based background segmentation with U2Net - (b6d5793) - Neale Swinnerton
- (**camera**) support device paths and symlinks for camera identification - (b3e6d19) - Neale Swinnerton
- (**capture**) implement atomic file writes using tempfile crate - (2caf24f) - Neale Swinnerton
- (**chyron**) format large diff stats with k/M suffixes - (7289478) - Neale Swinnerton
- (**chyron**) make chyron overlay optional with config and CLI flags - (d9ef580) - Neale Swinnerton
- (**chyron**) Change info line color to grey - (7a85c28) - Neale Swinnerton
- (**chyron**) Add transparent background and SHA display - (95a56a7) - Neale Swinnerton
- (**chyron**) Add colorized git stats to right side - (dd6b389) - Neale Swinnerton
- (**chyron**) Improve conventional commit display - (842a5b2) - Neale Swinnerton
- (**config**) Add XDG-compliant configuration file support - (d46c3b1) - Neale Swinnerton
- (**image**) Center person in frame using mask center of mass - (21fb058) - Neale Swinnerton
- (**metadata**) add parsed diff stats and configurable gallery title - (15e499d) - Neale Swinnerton
- (**metadata**) embed commit metadata in PNG files - (5a770b1) - Neale Swinnerton
- (**nix**) Add flake.nix - (649bae9) - Neale Swinnerton
- (**segmentation**) add MD5 checksum verification for model downloads - (e9d903a) - Neale Swinnerton
- (**server**) add web gallery viewer with carousel interface - (e9755d0) - Neale Swinnerton
- Add per-element font configuration with fallback - (dc1ee6f) - Neale Swinnerton
- Use fontconfig for font resolution by name - (fb05de1) - Neale Swinnerton
- Add XDG-compliant background image path resolution - (f39780b) - Neale Swinnerton
- add chyron overlay and conventional commit parsing - (d19eace) - Neale Swinnerton
- initial implementation of lolcommits-rs - (22cac04) - Neale Swinnerton
#### Bug Fixes
- (**packaging**) correct repository URL in PKGBUILD - (e537631) - Neale Swinnerton
#### Revert
- Restore edition 2024 and use let-chains syntax - (8d9d139) - Neale Swinnerton
#### Documentation
- expand configuration section with detailed options and background path resolution - (0e8630a) - Neale Swinnerton
- update README for binary rename and tmpfiles config location - (e0b7abb) - Neale Swinnerton
- Add sample configuration files - (63a45ff) - Neale Swinnerton
#### Tests
- Add unit tests for git, image_processor, and segmentation modules - (858a1fa) - Neale Swinnerton
#### Build system
- add Arch Linux packaging and release automation - (e44007f) - Neale Swinnerton
- Use rust-overlay to read rust-toolchain.toml - (1178eea) - Neale Swinnerton
- Add cargo-llvm-cov support to flake.nix - (e7618a4) - Neale Swinnerton
#### Refactoring
- (**arch**) move image processing from client to server - (10feb4f) - Neale Swinnerton
- (**build**) Reorganize flake.nix dependencies by purpose - (9d1957a) - Neale Swinnerton
- (**cli**) use --chyron and --no-chyron flags for chyron control - (0e287d4) - Neale Swinnerton
- (**config**) restructure configuration into logical sections - (d5ccd53) - Neale Swinnerton
- (**error**) eliminate string interpolation in error variants - (91e3060) - Neale Swinnerton
- (**error**) simplify Display implementation using Debug formatter - (aa69673) - Neale Swinnerton
- (**error**) simplify Result and Error types with wildcard imports - (e8c451d) - Neale Swinnerton
- (**git**) encapsulate diff stats in DiffStats and CommitMetadata structs - (085b230) - Neale Swinnerton
- (**git**) use --numstat for reliable diff stat parsing - (509fec4) - Neale Swinnerton
- (**segmentation**) improve model download error handling and validation - (c6ab260) - Neale Swinnerton
- extract server and capture modules from binaries - (d3ff145) - Neale Swinnerton
- rename lolcommits-rs to lolcommits throughout codebase - (b36259b) - Neale Swinnerton
- Move model to XDG_CACHE_HOME and use xdg crate consistently - (b543ca2) - Neale Swinnerton
- Rename blur_background to replace_background - (b153abb) - Neale Swinnerton
#### Miscellaneous Chores
- (**lint**) apply cargo fmt and clippy fixes - (c8d0ec1) - Neale Swinnerton
- (**lint**) apply cargo fmt and clippy fixes - (5f6081e) - Neale Swinnerton
- Remove .claude/settings.local.json from version control - (993d42e) - Neale Swinnerton

- - -

Changelog generated by [cocogitto](https://github.com/cocogitto/cocogitto).