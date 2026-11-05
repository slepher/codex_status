#!/usr/bin/env bash
# Linux host -> Windows MSVC portable build. Does not run the Windows application.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
: "${CODEX_STATUS_ARDUINOJSON:?Set to an ArduinoJson v7.4.3 src directory}"
headers=$(cd "$CODEX_STATUS_ARDUINOJSON" && pwd)
image="${CODEX_STATUS_XWIN_IMAGE:-messense/cargo-xwin@sha256:9856b895265d4966f212228ba64802cf89337e2a2a537aa2533c1b8784cbc81b}"
docker run --rm \
  -v "$repo:/io" -v "$headers:/arduinojson:ro" -w /io/bridge \
  -e CODEX_STATUS_ARDUINOJSON=/arduinojson \
  -e CARGO_TARGET_DIR=/io/artifacts/release-ci/cross-target \
  "$image" cargo xwin build --locked --release --target x86_64-pc-windows-msvc -p bridge-app
python3 - "$repo" <<'PY'
import hashlib, pathlib, struct, sys, zipfile
repo = pathlib.Path(sys.argv[1])
exe = repo / 'artifacts/release-ci/cross-target/x86_64-pc-windows-msvc/release/bridge-app.exe'
data = exe.read_bytes()
assert data[:2] == b'MZ', 'Missing DOS executable header'
offset = struct.unpack_from('<I', data, 0x3c)[0]
assert data[offset:offset+4] == b'PE\0\0', 'Missing PE header'
assert struct.unpack_from('<H', data, offset+4)[0] == 0x8664, 'Expected x64 PE'
output = repo / 'artifacts/release-ci/cross-dist'
output.mkdir(parents=True, exist_ok=True)
archive = output / 'codex-status-bridge-windows-x64-cross.zip'
files = [(exe, 'bridge-app.exe'), (repo / 'LICENSE', 'LICENSE'), (repo / 'README.md', 'README.md')]
templates = repo / 'tools/test-bridge/templates'
files += [(p, 'seed/templates/' + p.relative_to(templates).as_posix()) for p in sorted(templates.rglob('*')) if p.is_file()]
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
    for path, name in files:
        z.write(path, name)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None, 'ZIP CRC mismatch'
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix('.zip.sha256').write_text(f'{digest}  {archive.name}\n')
print(f'{archive}: {archive.stat().st_size} bytes; SHA256 {digest}')
PY
