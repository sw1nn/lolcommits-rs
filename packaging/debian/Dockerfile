# Builder image for the Ubuntu package.
#
# The base must match the deployment target (vps00, Ubuntu 26.04): the daemon
# links Ubuntu's libopencv-*410 and libfontconfig1 dynamically, so building
# anywhere else is what forced the patchelf-the-interpreter workaround this
# package exists to remove.
#
# The toolchain is not pinned here. rustup honours rust-toolchain.toml from the
# source tree at build time, so the pin stays in one place.
FROM ubuntu:26.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install --yes --no-install-recommends \
      build-essential pkg-config \
      clang libclang-dev llvm-dev \
      libopencv-dev \
      libfontconfig1-dev \
      libv4l-dev \
      libssl-dev \
      zlib1g-dev \
      git curl ca-certificates \
      dpkg-dev file \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --default-toolchain none
ENV PATH=/root/.cargo/bin:$PATH

COPY build-deb.sh /usr/local/bin/build-deb.sh
RUN chmod 0755 /usr/local/bin/build-deb.sh

ENTRYPOINT ["/usr/local/bin/build-deb.sh"]
