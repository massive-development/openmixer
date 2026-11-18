# The Linux packaging image for `tools/package.sh --docker`: Debian 12 (glibc 2.36) with Node 22,
# the Rust toolchain from rust-toolchain.toml (installed on first use by rustup) and the libraries
# the binary and the Tauri bundler need.
FROM node:22-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        libasound2-dev pkg-config libclang-dev clang libdbus-1-dev libssl-dev \
        libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev \
        file xdg-utils rpm \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain none

# The AppImage tools are AppImages themselves; there is no FUSE in a container.
ENV APPIMAGE_EXTRACT_AND_RUN=1

WORKDIR /src
