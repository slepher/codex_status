"""Verify one ROM's embedded identity and both OTA slot budgets before staging.

Git tags identify source snapshots; ROM versions remain per-target.
No device access, factory image, or partition write occurs here.
"""
import argparse
import csv
import hashlib
import json
import pathlib
import re
import shutil
import zipfile

TARGETS = {
    "zectrix-note4-b": ("zectrix-note4-400x300", "partitions_note4.csv", r"[0-9]+\.[0-9]+\.[0-9]+-note4"),
    "esp32-s3-epaper-154g": ("codex-status-154g", "partitions.csv", r"[0-9]+\.[0-9]+\.[0-9]+-bw(?:-[a-z0-9]+)?"),
}


def inspect_rom(data, env, partition_text):
    target, _, pattern = TARGETS[env]
    identities = set(re.findall(rb"codex-status-ota-v1\|([^|\x00]+)\|([^\x00]+)\x00", data))
    if len(identities) != 1:
        raise ValueError("ROM must contain exactly one distinct target/version marker")
    actual, version = (part.decode("ascii") for part in identities.pop())
    if actual != target or not re.fullmatch(pattern, version):
        raise ValueError(f"ROM identity mismatch: {actual}/{version} for {env}")
    slots = {}
    for row in csv.reader(line for line in partition_text.splitlines() if line.strip() and not line.lstrip().startswith("#")):
        row = [value.strip() for value in row]
        if row[1] == "app" and row[2] in ("ota_0", "ota_1"):
            slots[row[2]] = int(row[4], 0)
    if set(slots) != {"ota_0", "ota_1"} or len(data) > min(slots.values()):
        raise ValueError("ROM does not fit both native OTA slots")
    return dict(env=env, firmware_target=target, version=version, bytes=len(data),
                sha256=hashlib.sha256(data).hexdigest(), ota_slots=slots)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--env", choices=TARGETS, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    build = root / ".pio/build" / args.env
    info = inspect_rom((build / "firmware.bin").read_bytes(), args.env,
                       (root / TARGETS[args.env][1]).read_text())
    info["elf_sha256"] = hashlib.sha256((build / "firmware.elf").read_bytes()).hexdigest()
    usb_files = {name: build / name for name in
                 ["bootloader.bin", "partitions.bin", "firmware.bin", "firmware.elf"]}
    logical = "note4" if args.env == "zectrix-note4-b" else "154g"
    usb_files["boot_app0.bin"] = (root / ".pio-pkgs" / logical /
        "framework-arduinoespressif32/tools/partitions/boot_app0.bin")
    info["usb_files"] = {name: {"bytes": path.stat().st_size,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for name, path in usb_files.items()}
    args.out.mkdir(parents=True, exist_ok=True)
    name = f"codex-status-{info['version']}"
    shutil.copyfile(build / "firmware.bin", args.out / f"{name}.bin")
    (args.out / f"{name}.json").write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    with zipfile.ZipFile(args.out / f"{name}-usb.zip", "w", compression=zipfile.ZIP_DEFLATED) as kit:
        for item, path in usb_files.items():
            kit.write(path, item)
        kit.write(root / TARGETS[args.env][1], "partitions.csv")
        kit.writestr("manifest.json", json.dumps(info, indent=2) + "\n")
        kit.writestr("README.txt", "Match MAC/chip/Flash/target and back up first. Derive write offsets from the matching partition table and ESP32-S3 upload recipe. Do not erase all Flash or copy another target's offsets. See docs/build-install.md.\n")
    print(json.dumps(info, indent=2))


if __name__ == "__main__":
    main()
