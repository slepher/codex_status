#!/bin/bash
# Native production-HTML rendering/interaction checks. IPC is synthetic, no device access.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
output_root="$repo_root/artifacts/bridge-macos/wifi-ui"
mkdir -p "$output_root"
swiftc "$repo_root/tools/test-wifi-ui.swift" -o "$output_root/test-wifi-ui"
"$output_root/test-wifi-ui" "$repo_root/bridge/crates/app/ui/index.html" "$repo_root/tools/wifi-ui-scenarios.js" "$output_root"
