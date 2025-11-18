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

# Check if SSH agent is running
if [ -z "$SSH_AUTH_SOCK" ]; then
  echo "Error: SSH agent not running or SSH_AUTH_SOCK not set"
  echo "Please start ssh-agent and add your key:"
  echo "  eval \$(ssh-agent)"
  echo "  ssh-add"
  exit 1
fi

# Change to the directory containing this script
cd "$(dirname "$0")"

echo "Building lolcommits packages in clean chroot..."
echo "Chroot location: $CHROOT_DIR"
echo "SSH agent socket: $SSH_AUTH_SOCK"
echo ""

# Bind mount the SSH agent socket into the chroot
# The socket path needs to exist in the chroot, so we create the parent directory
CHROOT_SOCK_DIR="$CHROOT_DIR/root/$(dirname "$SSH_AUTH_SOCK")"
sudo mkdir -p "$CHROOT_SOCK_DIR"
sudo mount --bind "$SSH_AUTH_SOCK" "$CHROOT_DIR/root/$SSH_AUTH_SOCK" 2>/dev/null || true

# Cleanup function to unmount on exit
cleanup() {
  echo "Cleaning up bind mounts..."
  sudo umount "$CHROOT_DIR/root/$SSH_AUTH_SOCK" 2>/dev/null || true
}
trap cleanup EXIT

# Run the build with SSH_AUTH_SOCK passed through
# Use --preserve-env to pass through SSH_AUTH_SOCK
# Note: This requires sudoers configuration. Add to /etc/sudoers.d/makechrootpkg:
#   Defaults!/usr/bin/makechrootpkg env_keep += "SSH_AUTH_SOCK"
#   swn ALL=(ALL) NOPASSWD: /usr/bin/makechrootpkg
sudo --preserve-env=SSH_AUTH_SOCK makechrootpkg -c -r "$CHROOT_DIR" "$@"

echo ""
echo "Build complete! Package files:"
ls -lh *.pkg.tar.zst 2>/dev/null || echo "No package files found"
