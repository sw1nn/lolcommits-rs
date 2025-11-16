#!/bin/bash
# Build script for lolcommits packages using a clean chroot
# This ensures the package builds against system libraries, not Nix

set -e

CHROOT_DIR="${CHROOT_DIR:-/var/lib/archbuild/extra-x86_64}"

if [ ! -d "$CHROOT_DIR/root" ]; then
    echo "Error: Chroot not found at $CHROOT_DIR/root"
    echo "Please run: sudo mkarchroot $CHROOT_DIR/root base-devel"
    exit 1
fi

# Change to the directory containing this script
cd "$(dirname "$0")"

echo "Building lolcommits packages in clean chroot..."
echo "Chroot location: $CHROOT_DIR"
echo ""

# Run the build
sudo makechrootpkg -c -r "$CHROOT_DIR" "$@"

echo ""
echo "Build complete! Package files:"
ls -lh *.pkg.tar.zst 2>/dev/null || echo "No package files found"
