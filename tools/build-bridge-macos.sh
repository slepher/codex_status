#!/bin/bash
# Local native app build; uses the system Rust toolchain. No firmware build.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
build_root="$repo_root/artifacts/bridge-macos"
target_root="$build_root/target"
app_root="$build_root/Codex Status Bridge.app"
: "${CODEX_STATUS_ARDUINOJSON:?Set CODEX_STATUS_ARDUINOJSON to a pinned ArduinoJson src directory}"
cd "$repo_root/bridge"
cargo build --locked -p bridge-app --target-dir "$target_root"
python3 - "$repo_root" "$target_root" "$app_root" <<'PY'
import pathlib, plistlib, shutil, sys
repo, target, app = map(pathlib.Path, sys.argv[1:])
contents = app / 'Contents'
(contents / 'MacOS').mkdir(parents=True, exist_ok=True)
(contents / 'Resources').mkdir(exist_ok=True)
shutil.copy2(target / 'debug/bridge-app', contents / 'MacOS/bridge-app')
shutil.copytree(repo / 'tools/test-bridge/templates', contents / 'Resources/seed/templates', dirs_exist_ok=True)
with (repo / 'bridge/crates/app/Info.plist').open('rb') as f:
    info = plistlib.load(f)
info.update(CFBundleIdentifier='dev.codexstatus.bridge', CFBundleName='Codex Status Bridge',
            CFBundleExecutable='bridge-app', CFBundlePackageType='APPL', CFBundleShortVersionString='0.1.0',
            CFBundleVersion='1', LSMinimumSystemVersion='14.0', NSHighResolutionCapable=True)
with (contents / 'Info.plist').open('wb') as f:
    plistlib.dump(info, f)
PY
codesign --force --sign - "$app_root"
printf '%s\n' "$app_root"
