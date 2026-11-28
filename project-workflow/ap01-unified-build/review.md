# Review

Passed: both variants use native_build_0041.py and the same firmware linker/packager; App configuration contains only paths within its own source root; old scripts have no subprocess, architecture flags, platform source lists or link commands. Existing budget fields remain available, and changed source/object/config invalidates staged linking. The ROM packager additionally checks linked ELF and linker-script identities.

Standalone inputs remain within firmware. Firmware never discovers App/reverse checkouts by default. Runtime/maintenance, storage formats, recovery patches and device deployment are unchanged by this build refactor.

Validation evidence and candidate hashes: status.md. Optional Unicorn instruction test skipped; hardware/simulator acceptance not claimed. Existing App compiler warnings remain.

## 2026-11-28 App 功能命名

检查独立 App 排除驻留 ROM manifest 的行为，并核对版本的设备和启动身份。
