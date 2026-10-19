#!/usr/bin/env bash
set -euo pipefail
TASK_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TASK_ARCH="${1:-arm64}"
mkdir -p "$TASK_ROOT/.cache" "$TASK_ROOT/runtime"
curl -fL --retry 3 https://github.com/libsdl-org/SDL/releases/download/release-3.4.18/SDL3-3.4.18.tar.gz -o "$TASK_ROOT/.cache/SDL3.tar.gz"
echo "9c75cf16330322c217dedd2e0609f1124f1b54b8633e763467b4684d0f4334a3  $TASK_ROOT/.cache/SDL3.tar.gz" | shasum -a 256 -c -
tar -xzf "$TASK_ROOT/.cache/SDL3.tar.gz" -C "$TASK_ROOT/.cache"
cmake -S "$TASK_ROOT/.cache/SDL3-3.4.18" -B "$TASK_ROOT/.cache/sdl-build-$TASK_ARCH" -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES="$TASK_ARCH" -DCMAKE_OSX_DEPLOYMENT_TARGET=13.0 -DSDL_SHARED=ON -DSDL_STATIC=OFF -DSDL_TEST_LIBRARY=OFF -DSDL_TESTS=OFF -DSDL_EXAMPLES=OFF
cmake --build "$TASK_ROOT/.cache/sdl-build-$TASK_ARCH" --config Release -j 3
cp "$TASK_ROOT/.cache/sdl-build-$TASK_ARCH/libSDL3.0.dylib" "$TASK_ROOT/runtime/libSDL3.0.dylib"

