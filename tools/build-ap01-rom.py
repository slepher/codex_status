"""Compatibility entry for firmware-owned AP01 ROM packaging.
Application build/test orchestration may import assemble/validate_candidate.
All ROM layout, menu patching and OTA/recovery image policy live in firmware.
"""
from pathlib import Path
import runpy
_builder=Path(__file__).resolve().parents[1].parent/'cuktech-ap01-firmware/tooling/scripts/build-native-app-rom-0041.py'
globals().update({k:v for k,v in runpy.run_path(str(_builder)).items() if not k.startswith('__')})
if __name__ == '__main__':main()
