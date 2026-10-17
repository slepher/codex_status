#!/usr/bin/env bash
# Build directories must already exist (AGENTS.md: pre-create with elevation).
set -euo pipefail
if [[ $# != 2 ]]; then
    echo "Usage: $0 <existing-qemu-v10.1.0-source> <existing-build-directory>" >&2
    exit 2
fi
source_dir=$(realpath "$1")
build_dir=$(realpath "$2")
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
[[ -d "$source_dir" && -d "$build_dir" ]]
if [[ -f "$build_dir/build.ninja" ]]; then
    python3 -B - "$source_dir" "$build_dir" <<'PY'
import json
from pathlib import Path
import sys
source, build = map(Path, sys.argv[1:])
info = json.loads((build / 'meson-info/meson-info.json').read_text())
if Path(info['directories']['source']).resolve() != source.resolve():
    sys.exit('Existing Meson build belongs to another source tree; refusing reuse')
config = dict(line.split('=', 1) for line in (build / 'config-host.mak').read_text().splitlines()
              if '=' in line and not line.startswith('#'))
if config.get('TARGET_DIRS') != 'riscv32-softmmu':
    sys.exit('Existing build must target only riscv32-softmmu; refusing reuse')
PY
fi
[[ $(git -C "$source_dir" rev-parse HEAD) == f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3 ]] || {
    echo 'Expected QEMU v10.1.0 commit f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3' >&2
    exit 2
}
cp -- "$script_dir/qemu-ap01.c" "$source_dir/hw/riscv/ap01.c"
cp -- "$script_dir/qemu-ap01-uart.h" "$source_dir/hw/riscv/qemu-ap01-uart.h"
cp -- "$script_dir/qemu-ap01-spi.h" "$source_dir/hw/riscv/qemu-ap01-spi.h"
cp -- "$script_dir/qemu-ap01-dma.h" "$source_dir/hw/riscv/qemu-ap01-dma.h"
cp -- "$script_dir/qemu-ap01-pwm.h" "$source_dir/hw/riscv/qemu-ap01-pwm.h"
cp -- "$script_dir/qemu-ap01-pec.h" "$source_dir/hw/riscv/qemu-ap01-pec.h"
cp -- "$script_dir/qemu-ap01-flash.h" "$source_dir/hw/riscv/qemu-ap01-flash.h"
cp -- "$script_dir/qemu-ap01-timer.h" "$source_dir/hw/riscv/qemu-ap01-timer.h"
cp -- "$script_dir/qemu-ap01-rf.h" "$source_dir/hw/riscv/qemu-ap01-rf.h"
cp -- "$script_dir/qemu-ap01-rf-cal.h" "$source_dir/hw/riscv/qemu-ap01-rf-cal.h"
cp -- "$script_dir/qemu-ap01-rf-acal.h" "$source_dir/hw/riscv/qemu-ap01-rf-acal.h"
cp -- "$script_dir/qemu-ap01-rf-pm.h" "$source_dir/hw/riscv/qemu-ap01-rf-pm.h"
cp -- "$script_dir/qemu-ap01-trng.h" "$source_dir/hw/riscv/qemu-ap01-trng.h"
cp -- "$script_dir/qemu-ap01-mac.h" "$source_dir/hw/riscv/qemu-ap01-mac.h"
cp -- "$script_dir/qemu-ap01-machw.h" "$source_dir/hw/riscv/qemu-ap01-machw.h"
python3 -B "$script_dir/apply-qemu-ap01-cpu.py" --source "$source_dir"
python3 -B - "$source_dir/hw/riscv/meson.build" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
line = "riscv_ss.add(files('ap01.c'))"
if line not in s.splitlines():
    p.write_text(s + '\n' + line + '\n')
PY
cd -- "$build_dir"
if [[ ! -f build.ninja ]]; then
    "$source_dir/configure" --target-list=riscv32-softmmu \
        --disable-docs --disable-werror --disable-tools --disable-guest-agent \
        --disable-user --disable-gtk --disable-sdl --disable-opengl \
        --disable-vnc --disable-curses --without-default-devices
fi
ninja -j 8 qemu-system-riscv32
./qemu-system-riscv32 --version
sha256sum "$script_dir/qemu-ap01.c" ./qemu-system-riscv32
