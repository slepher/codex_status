#!/usr/bin/env bash
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$repo/artifacts/external-rtc}"
mkdir -p "$out"
for target in note4 154g; do
    flag=-UCODEX_TARGET_NOTE4
    if [[ "$target" == note4 ]]; then flag=-DCODEX_TARGET_NOTE4=1; fi
    "${CXX:-clang++}" -std=c++17 -Wall -Wextra -Werror -g -fsanitize=address,undefined \
        -I"$repo/src" -I"$repo/tools/rtc-test-stubs" "$flag" \
        "$repo/tools/test-external-rtc.cpp" "$repo/src/external_rtc.cpp" -o "$out/test-$target"
    "$out/test-$target"
done
