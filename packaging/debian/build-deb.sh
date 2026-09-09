#!/usr/bin/env bash
# Builds the lolcommits-server .deb. Runs INSIDE the builder container.
#
#   build-deb.sh <version> <uid:gid>
#
# Reads the source read-only from /src and writes the package to /out.
set -euo pipefail

version=${1:?version required, e.g. 6.0.0-1}
owner=${2:?uid:gid required}

pkgname=lolcommits-server
arch=amd64

# Copy rather than build in /src: the mount is read-only, and building as root
# in the working tree would leave root-owned artefacts behind.
build=/build
cp -a /src "$build"
cd "$build"

cargo build --release --locked

stage=/tmp/stage
rm -rf "$stage"
install -Dm0755 target/release/lolcommitsd "$stage/usr/bin/lolcommitsd"
install -Dm0644 -t "$stage/usr/share/lolcommits/static" assets/static/*

# Derive Depends from the binary rather than hardcoding a library list. A
# soname change (an OpenCV major bump, say) then fails the build here instead
# of producing a package that installs and will not load - which is the exact
# failure the old build-on-Arch-and-patchelf path could not detect.
#
# Run from a scratch dir: dpkg-shlibdeps needs a debian/control relative to the
# working directory, and that directory must not end up inside the package.
shlibdir=$(mktemp -d)
mkdir -p "$shlibdir/debian"
printf 'Source: %s\n\nPackage: %s\nArchitecture: %s\n' "$pkgname" "$pkgname" "$arch" \
    > "$shlibdir/debian/control"
depends=$(cd "$shlibdir" && dpkg-shlibdeps -O "$stage/usr/bin/lolcommitsd" \
    | sed 's/^shlibs:Depends=//')

if [ -z "$depends" ]; then
    echo "ERROR: dpkg-shlibdeps produced no dependencies" >&2
    exit 1
fi
echo "Depends: $depends"

install -d "$stage/DEBIAN"
sed -e "s/@VERSION@/$version/" -e "s/@DEPENDS@/$depends/" \
    /src/packaging/debian/control.in > "$stage/DEBIAN/control"

deb="/out/${pkgname}_${version}_${arch}.deb"
dpkg-deb --build --root-owner-group "$stage" "$deb"

# Checksum is published alongside the package: the Ansible download verifies
# against it, and get_url cannot fetch a checksum URL that needs auth headers.
( cd /out && sha256sum "$(basename "$deb")" > "$(basename "$deb").sha256" )

chown "$owner" "$deb" "$deb.sha256"

echo
dpkg-deb --info "$deb"
dpkg-deb --contents "$deb"
