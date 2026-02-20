FROM ubuntu:22.04

# Prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install base dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    # Linux aarch64 musl cross-compiler
    musl-tools \
    # macOS cross-compilation dependencies (osxcross)
    libxml2-dev \
    libssl-dev \
    zlib1g-dev \
    llvm-dev \
    clang \
    cmake \
    python3 \
    # Windows cross-compilation (MinGW)
    gcc-mingw-w64-x86-64 \
    g++-mingw-w64-x86-64 \
    # Misc
    jq \
    xz-utils \
    zip \
    && rm -rf /var/lib/apt/lists/*

# Install the aarch64 musl cross-compiler.
# musl-tools only provides x86_64-linux-musl-gcc; for aarch64 we need the
# musl-cross-make toolchain.
RUN curl -sSL https://musl.cc/aarch64-linux-musl-cross.tgz | tar xz -C /opt
ENV PATH="/opt/aarch64-linux-musl-cross/bin:${PATH}"

# Install Rust (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

# Add all Rust targets we need to build for
RUN rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    x86_64-apple-darwin \
    aarch64-apple-darwin \
    x86_64-pc-windows-gnu

# Configure Cargo cross-compilation linkers
# These are consulted by cargo when building for non-host targets.
RUN mkdir -p /root/.cargo && cat >> /root/.cargo/config.toml <<'EOF'
[target.x86_64-unknown-linux-musl]
linker = "musl-gcc"

[target.aarch64-unknown-linux-musl]
linker = "aarch64-linux-musl-gcc"

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"

# macOS linkers are set dynamically in build.sh once osxcross is built,
# because the Darwin version suffix in the binary names depends on the SDK.
EOF

# Build osxcross with the macOS 11.3 SDK.
# The SDK tarball must be placed at docker-build/sdk/MacOSX11.3.sdk.tar.xz
# before running `docker build` (it is .gitignored).
RUN git clone https://github.com/tpoechtrager/osxcross /opt/osxcross
COPY sdk/MacOSX11.3.sdk.tar.xz /opt/osxcross/tarballs/
WORKDIR /opt/osxcross
RUN UNATTENDED=1 ./build.sh

ENV PATH="/opt/osxcross/target/bin:${PATH}"
# Tell the macOS linker wrappers where the SDK lives
ENV OSXCROSS_SDK_VERSION=11.3
ENV MACOSX_DEPLOYMENT_TARGET=11.3

WORKDIR /workspace
