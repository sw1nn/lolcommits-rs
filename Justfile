default:
    @just --list

# Refuse to release from any branch other than main.
_assert-main:
    #!/usr/bin/env bash
    set -euo pipefail
    branch=$(git branch --show-current)
    if [ "$branch" != "main" ]; then
        echo "ERROR: releases can only be run from main (currently on '$branch')" >&2
        exit 1
    fi

# publish-deb runs last and pulls in package-deb. It is part of the release
# because the deployment pins the Debian version and installs it from the
# release asset: bumping without publishing the .deb leaves the pin naming a
# package that cannot be fetched, which surfaces only when a converge fails.
#
# A failure after `package` leaves the Arch upload and the pushed tag in place.
# Both `package` and `publish-deb` re-run standalone against the version in the
# PKGBUILD, so recovery is to fix the cause and re-run the failed recipe.

# Bump the version, tag and push (see cog.toml), then build and upload packages.
release type='auto': _assert-main && package publish-deb
    cog bump --{{ type }}

# Build the Arch packages in a clean chroot, verify the version, then upload.
package:
    #!/usr/bin/env bash
    # The version comes from the PKGBUILD rather than an argument, so this is
    # safe to re-run standalone after a failed or interrupted release.
    set -euo pipefail
    pkgver=$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD)
    sw1nn-makepkg-chroot -C packaging/arch
    sw1nn-pkg-ctl upload packaging/arch/*-"$pkgver"-*.pkg.tar.zst

# Build the Ubuntu .deb in a container matching the deployment target.
package-deb:
    #!/usr/bin/env bash
    # The version comes from the PKGBUILD, as `package` does, so this is safe
    # to re-run standalone after a failed or interrupted release.
    set -euo pipefail
    pkgver=$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD)
    pkgrel=$(sed -n 's/^pkgrel=//p' packaging/arch/PKGBUILD)
    version="${pkgver}-${pkgrel}"
    mkdir -p packaging/debian/out
    docker build --tag lolcommits-deb-builder packaging/debian
    # Source read-only; the container copies it before building. Passing the
    # caller's ids lets it hand back artefacts we own rather than root's.
    docker run --rm \
        --volume "$PWD":/src:ro \
        --volume "$PWD/packaging/debian/out":/out \
        lolcommits-deb-builder "$version" "$(id -u):$(id -g)"

# Attach the .deb and its checksum to the Forgejo release for this tag.
publish-deb: package-deb
    #!/usr/bin/env bash
    # Forgejo requires authentication on every route used here, so this needs a
    # token with write:repository in the Secret Service.
    set -euo pipefail
    pkgver=$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD)
    pkgrel=$(sed -n 's/^pkgrel=//p' packaging/arch/PKGBUILD)
    version="${pkgver}-${pkgrel}"
    tag="v${pkgver}"
    api="https://code.sw1nn.net/api/v1/repos/sw1nn/lolcommits-rs"
    deb="packaging/debian/out/lolcommits-server_${version}_amd64.deb"

    token=$(secret-tool lookup URL https://code.sw1nn.net UserName release_publish_token)
    if [ -z "$token" ]; then
        echo "ERROR: no release_publish_token in the Secret Service" >&2
        exit 1
    fi
    auth=(--header "Authorization: token ${token}")

    # Reuse the release if the tag already has one, so this is re-runnable.
    id=$(curl -sf "${auth[@]}" "${api}/releases/tags/${tag}" | jq -r '.id // empty' || true)
    if [ -z "$id" ]; then
        id=$(curl -sf "${auth[@]}" --header 'Content-Type: application/json' \
                --data "$(jq -nc --arg t "$tag" '{tag_name: $t, name: $t}')" \
                "${api}/releases" | jq -r .id)
        echo "created release ${tag} (${id})"
    else
        echo "reusing release ${tag} (${id})"
    fi

    for f in "$deb" "${deb}.sha256"; do
        name=$(basename "$f")
        # Replace an existing asset of the same name, so a re-run is not additive.
        old=$(curl -sf "${auth[@]}" "${api}/releases/${id}/assets" \
                | jq -r --arg n "$name" '.[] | select(.name == $n) | .id // empty' || true)
        if [ -n "$old" ]; then
            curl -sf "${auth[@]}" --request DELETE "${api}/releases/${id}/assets/${old}" >/dev/null
        fi
        curl -sf "${auth[@]}" --form "attachment=@${f}" \
            "${api}/releases/${id}/assets?name=${name}" >/dev/null
        echo "uploaded ${name}"
    done
