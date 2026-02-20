# Cross-compilation Docker image

This directory contains the Dockerfile for a build image that can cross-compile
`wasm-bodge` for all supported platforms from a single Linux container:

| Target | OS | Notes |
|---|---|---|
| `x86_64-unknown-linux-musl` | Linux x86_64 | Static musl binary |
| `aarch64-unknown-linux-musl` | Linux ARM64 | Static musl binary |
| `x86_64-apple-darwin` | macOS Intel | via osxcross |
| `aarch64-apple-darwin` | macOS Apple Silicon | via osxcross |
| `x86_64-pc-windows-gnullvm` | Windows x86_64 | via llvm-mingw |
| `aarch64-pc-windows-gnullvm` | Windows ARM64 | via llvm-mingw |

The built image is pushed to `ghcr.io/alexjg/wasm-bodge-cross` and used by the
`release-binaries` GitHub Actions workflow. You can also use it locally to
produce release archives without needing native toolchains installed.

## Prerequisites

- Docker
- The macOS 11.3 SDK tarball (`MacOSX11.3.sdk.tar.xz`)

Due to Apple's licensing restrictions the SDK cannot be committed to this
repository or distributed in a public Docker image.

## Building the image

### Step 1 — Obtain the macOS SDK

On a Mac with Xcode installed:

```bash
cd $(xcode-select -p)/Platforms/MacOSX.platform/Developer/SDKs
tar -cJf ~/MacOSX11.3.sdk.tar.xz MacOSX11.3.sdk
```

### Step 2 — Place the SDK in the build context

```bash
mkdir -p docker-build/sdk
cp ~/MacOSX11.3.sdk.tar.xz docker-build/sdk/
```

The `sdk/` directory is git-ignored, so the SDK will not be committed.

### Step 3 — Build the image

```bash
cd docker-build
docker build -t wasm-bodge-cross:latest .
```

This takes 15–30 minutes the first time (osxcross compiles from source).

### Step 4 — Push to GitHub Container Registry

```bash
echo $GITHUB_TOKEN | docker login ghcr.io -u alexjg --password-stdin
docker tag wasm-bodge-cross:latest ghcr.io/alexjg/wasm-bodge-cross:latest
docker push ghcr.io/alexjg/wasm-bodge-cross:latest
```

### Step 5 — Make the package private

Apple's SDK license prohibits redistribution, so the image must be private:

1. Go to https://github.com/alexjg?tab=packages
2. Click `wasm-bodge-cross` → Package settings
3. Set visibility to **Private**

### Step 6 — Add secrets to the repository

The Actions workflow needs credentials to pull the private image:

| Secret | Value |
|---|---|
| `GHCR_USERNAME` | `alexjg` |
| `GHCR_TOKEN` | A PAT with `read:packages` scope |

Add them at: Settings → Secrets and variables → Actions

## Using the image locally

Build a specific target and produce a release archive:

```bash
./build-in-docker.sh x86_64-apple-darwin v0.1.0
```

This produces `wasm-bodge-v0.1.0-x86_64-apple-darwin.tar.gz` in your working
directory. Swap the target and tag as needed.

For an interactive shell inside the container:

```bash
docker run --rm -it \
  -v "$(pwd):/workspace" \
  -w /workspace \
  ghcr.io/alexjg/wasm-bodge-cross:latest \
  bash
```

## Updating llvm-mingw

The Windows ARM64 target (`aarch64-pc-windows-gnullvm`) uses
[llvm-mingw](https://github.com/mstorsjo/llvm-mingw) rather than GCC-based
MinGW, because GCC does not support aarch64 Windows. The version is currently
pinned to `20251021` and is hardcoded in three places — if you upgrade it you
must update all three consistently:

1. The `RUN curl` line in `Dockerfile` that downloads and unpacks the tarball
2. The `ENV` linker path directives in `Dockerfile` (e.g. `CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER`)
3. The `LLVM_MINGW_BIN` path in the `aarch64-pc-windows-gnullvm` case in `build.sh`

## Updating the image

1. Edit the `Dockerfile`
2. Rebuild and push with a new version tag:
   ```bash
   docker build -t wasm-bodge-cross:latest .
   docker tag wasm-bodge-cross:latest ghcr.io/alexjg/wasm-bodge-cross:v2
   docker push ghcr.io/alexjg/wasm-bodge-cross:latest
   docker push ghcr.io/alexjg/wasm-bodge-cross:v2
   ```
3. Update the image reference in `.github/workflows/release-binaries.yml`
