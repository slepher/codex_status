"""Compatibility dispatch only; build policy belongs to the firmware repository."""
from pathlib import Path
import runpy
import sys


def forward(stage):
    repo = Path(__file__).resolve().parents[1]
    entry = repo.parent / 'cuktech-ap01-firmware/tooling/scripts/build-firmware-0041.py'
    sys.path.insert(0, str(entry.parent))
    sys.argv = [str(entry), '--stage', stage,
                '--app-config', str(repo / 'ports/ap01/build-0041.json'),
                '--build-dir', str(repo / 'artifacts/ap01-platform-integration/device'), *sys.argv[1:]]
    runpy.run_path(str(entry), run_name='__main__')
