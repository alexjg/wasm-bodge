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

# Install llvm-mingw for Windows aarch64 (aarch64-pc-windows-gnullvm).
# The GCC-based MinGW doesn't support aarch64 Windows; llvm-mingw does.
# VERSION PINNED: if you update the version here you must also update the ENV
# linker paths below and the LLVM_MINGW_BIN path in build.sh. See README.md.
RUN curl -sSL https://github.com/mstorsjo/llvm-mingw/releases/download/20251021/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64.tar.xz \
    | tar xJ -C /opt
ENV PATH="/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin:${PATH}"

# Install Rust (stable)
# Pin RUSTUP_HOME and CARGO_HOME to absolute paths so they work even when
# GitHub Actions overrides HOME (it sets HOME=/github/home in containers).
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH="/usr/local/cargo/bin:${PATH}"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

# Add all Rust targets we need to build for
RUN rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    x86_64-apple-darwin \
    aarch64-apple-darwin \
    x86_64-pc-windows-gnullvm \
    aarch64-pc-windows-gnullvm

# Cross-compilation linkers as ENV directives. These are baked into the image
# and survive GitHub Actions overriding HOME/CARGO_HOME in container jobs.
# Absolute paths so we don't depend on PATH either.
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/bin/musl-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=/opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc
ENV CC_x86_64_unknown_linux_musl=/usr/bin/musl-gcc
ENV CC_aarch64_unknown_linux_musl=/opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc
# VERSION PINNED: if you update the llvm-mingw version in these paths you must
# also update the RUN curl line above and the LLVM_MINGW_BIN path in build.sh. See README.md.
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/x86_64-w64-mingw32-clang
ENV CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/aarch64-w64-mingw32-clang
ENV CC_x86_64_pc_windows_gnullvm=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/x86_64-w64-mingw32-clang
ENV CC_aarch64_pc_windows_gnullvm=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/aarch64-w64-mingw32-clang
ENV AR_x86_64_pc_windows_gnullvm=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/x86_64-w64-mingw32-ar
ENV AR_aarch64_pc_windows_gnullvm=/opt/llvm-mingw-20251021-ucrt-ubuntu-22.04-x86_64/bin/aarch64-w64-mingw32-ar
# macOS linkers are set dynamically in build.sh because the osxcross binary
# names include the Darwin version suffix (e.g. x86_64-apple-darwin20.4-clang).

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
