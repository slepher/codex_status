#!/usr/bin/env bash
# Reproducible freestanding RV32 OTA module; no device access or new directories.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo=$(realpath "$script_dir/../..")
out="$repo/artifacts/ap01-research/qemu-0041-20261017"
source_dir="$repo/firmware/ap01"
tool="$repo/../cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf"
prefix=${1:-ota-channel-linked-v1}
[[ "$prefix" =~ ^[a-zA-Z0-9_-]+$ && -d "$out" ]]
for suffix in channel.s channel.o channel.su port.s port.o port.su manifest.s manifest.o elf bin map; do
    [[ ! -e "$out/$prefix.$suffix" ]] || { echo 'Choose a new output prefix' >&2; exit 2; }
done
for unit in channel port; do
    if [[ "$unit" == channel ]]; then source_file=ota_channel.c; else source_file=ota_0041.c; fi
    "$tool-gcc" -B"$(dirname -- "$tool")/" -march=rv32imafc_zicsr -mabi=ilp32f \
        -Os -ffreestanding -fno-builtin -ffunction-sections -fdata-sections \
        -fno-pic -mno-relax -msmall-data-limit=0 -nostdlib -nostartfiles \
        -Wall -Wextra -Werror -fstack-usage -S "$source_dir/$source_file" \
        -o "$out/$prefix.$unit.s"
    "$tool-as" -march=rv32imafc_zicsr -mabi=ilp32f \
        "$out/$prefix.$unit.s" -o "$out/$prefix.$unit.o"
done
cat > "$out/$prefix.manifest.s" <<'ASM'
.section .ota_manifest,"a"
.global ota_0041_manifest
ota_0041_manifest:
.zero 256
ASM
"$tool-as" -march=rv32imafc_zicsr -mabi=ilp32f \
    "$out/$prefix.manifest.s" -o "$out/$prefix.manifest.o"
"$tool-ld" -m elf32lriscv --gc-sections --no-relax -T "$source_dir/ota_0041.ld" \
    "$out/$prefix.channel.o" "$out/$prefix.port.o" "$out/$prefix.manifest.o" \
    -Map="$out/$prefix.map" -o "$out/$prefix.elf"
"$tool-objcopy" -O binary "$out/$prefix.elf" "$out/$prefix.bin"
[[ -z $("$tool-nm" -u "$out/$prefix.elf") ]]
sha256sum "$out/$prefix.elf" "$out/$prefix.bin" \
    "$source_dir/ota_channel.c" "$source_dir/ota_0041.c"
cat "$out/$prefix.channel.su" "$out/$prefix.port.su"
