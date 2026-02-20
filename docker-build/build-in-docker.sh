SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

docker run --rm \
  -v "$SCRIPT_DIR/../:/workspace" \
  -w /workspace \
  ghcr.io/alexjg/wasm-bodge-cross:latest \
  ./docker-build/build.sh "$@"
