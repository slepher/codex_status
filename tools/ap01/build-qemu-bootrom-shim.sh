#!/usr/bin/env bash
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo=$(realpath "$script_dir/../..")
out="$repo/artifacts/ap01-research/qemu-0041-20261017"
tool="$repo/../cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf"
sdk="$repo/../bouffalo_sdk"
prefix=${1:-qemu-bootrom-shim-v10}
[[ "$prefix" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo 'Invalid output prefix' >&2; exit 2; }
[[ -d "$out" ]]
for suffix in s o elf bin; do
    [[ ! -e "$out/$prefix.$suffix" ]] || {
        echo "Output exists; choose a new prefix: $out/$prefix.$suffix" >&2
        exit 2
    }
done
[[ $(git -C "$sdk" rev-parse HEAD) == 41bf338a57be53919d99f6c6715a96100428c97d ]]
"$tool-gcc" -B"$(dirname -- "$tool")/" -march=rv32imafc_zicsr -mabi=ilp32f -Os -ffreestanding -fno-builtin \
    -ffunction-sections -fdata-sections -fno-pic -mno-relax -msmall-data-limit=0 \
    -nostdlib -nostartfiles -Wall -Wextra -Werror -S \
    "$script_dir/qemu-bootrom-shim.c" -o "$out/$prefix.s"
# This relocatable toolchain's GCC resolves bare `as`; invoke its own tools.
"$tool-as" -march=rv32imafc_zicsr -mabi=ilp32f \
    "$out/$prefix.s" -o "$out/$prefix.o"
"$tool-ld" -m elf32lriscv --gc-sections --no-relax \
    -T "$script_dir/qemu-bootrom-shim.ld" "$out/$prefix.o" \
    -o "$out/$prefix.elf"
"$tool-objcopy" -O binary "$out/$prefix.elf" "$out/$prefix.bin"
python3 -B - "$out/$prefix.bin" <<'PY'
import struct, sys
from pathlib import Path
p = Path(sys.argv[1]); data = p.read_bytes()
table = struct.unpack('<512I', data[:2048])
assert {i for i, v in enumerate(table) if v} == {
    14,20,185,132,131,134,107,89,149,147,161,162,163,320,329,358,359,365,367,368,369,371,374,377,379,384,389,392,385,387,393,402,401,440,444,446,447,448,449,450}
assert struct.unpack_from('<4I', data, 0x800) == (0x43455041, 1, 128, 0)
pec = struct.unpack_from('<128I', data, 0x810)
assert {i for i, v in enumerate(pec) if v} == {x // 4 for x in (0x30,0x88,0x40,0x44,0x48,0x4c,0x50,0xe8)}
assert all(0x90002000 <= v < 0x90001000 + len(data) and v % 2 == 0
           for v in (*table, *pec) if v)
assert len(data) <= 0x10000
print(f'{len(data)} bytes; {sum(bool(v) for v in table)} BootROM entries and eight PEC transport entries validated')
PY
"$tool-readelf" -h -S "$out/$prefix.elf"
sha256sum "$out/$prefix.elf" "$out/$prefix.bin"
