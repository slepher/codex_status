"""Compatibility entry for firmware-owned AP01 ROM packaging.

The App supplies build paths; the firmware packager owns image policy.
"""
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location
import sys

repo = Path(__file__).resolve().parents[1]
spec = spec_from_file_location('ap01_rom_packager', repo.parent / 'cuktech-ap01-firmware/tooling/scripts/build-native-app-rom-0041.py')
_builder = module_from_spec(spec)
spec.loader.exec_module(_builder)
# Existing acceptance/QEMU callers import only this small, explicit surface.
assemble = _builder.assemble
validate_candidate = _builder.validate_candidate
sha = _builder.sha
FORMAT = _builder.FORMAT
ORIGINAL_SHA = _builder.ORIGINAL_SHA
main = _builder.main

if __name__ == '__main__':
    if not any(x == '--source-dir' or x.startswith('--source-dir=') for x in sys.argv):
        sys.argv += ['--source-dir', str(repo / 'artifacts/ap01-platform-integration/device')]
    if not any(x == '--tool-prefix' or x.startswith('--tool-prefix=') for x in sys.argv) and not any(x in sys.argv for x in ('-h', '--help')):
        prefix = _builder._native.tool_prefix()
        sys.argv += ['--tool-prefix', prefix]
    main()
