#!/usr/bin/env bash
# Bounded original-ROM iteration. Existing directories only; invoke elevated.
set -euo pipefail
if [[ $# -lt 1 ]]; then
    echo 'Usage: run-qemu-mock.sh <new-report.json> [extra runner arguments]' >&2
    exit 2
fi
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(cd -- "$script_dir/../.." && pwd)
cd -- "$repo_dir"
report=$1
shift
[[ -d $(dirname -- "$report") && ! -e "$report" && ! -e "$report.build.log" ]]
(set -o noclobber; bash "$script_dir/build-qemu-ap01.sh" \
    /home/slepher/qemu-ap01-0041/source /home/slepher/qemu-ap01-0041/build \
    > "$report.build.log" 2>&1)
python3 -B "$script_dir/run-qemu-ap01.py" \
    --qemu /home/slepher/qemu-ap01-0041/build/qemu-system-riscv32 \
    --image artifacts/ap01-research/ap01-1.0.2_0041.bin \
    --mhint-latch --mtvt-latch --cache-csr-latches --boot-mmio --efuse-shadow \
    --otp-fixture artifacts/ap01-research/qemu-0041-20261017/otp-synthetic-ap01.bin \
    --psram-mock --boot2-mock --run-to-trap --timeout 15 \
    --cpu rv32,x-ap01-p=on,xtheadba=on,xtheadbb=on,xtheadbs=on,xtheadcmo=on,xtheadcondmov=on,xtheadmac=on,xtheadmemidx=on,xtheadmempair=on,xtheadsync=on \
    --output "$report" "$@"
