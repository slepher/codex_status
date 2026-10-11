"""Exercise marker selection, target mismatch, and two-slot capacity guards."""
import importlib.util
import pathlib

root = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release_rom", root / "tools/stage-release-rom.py")
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)
for env, (target, partitions, _) in stage.TARGETS.items():
    suffix = "note4" if env == "zectrix-note4-b" else "bw-rtc1"
    marker = f"codex-status-ota-v1|{target}|0.18.84-{suffix}\0".encode()
    table = (root / partitions).read_text()
    assert stage.inspect_rom(marker, env, table)["firmware_target"] == target
    for data, changed_table in [(b"missing", table), (marker + marker.replace(b"0.18.84", b"0.18.85"), table),
                                (marker, "ota_0,app,ota_0,0x10000,1\nota_1,app,ota_1,0x20000,1")]:
        try:
            stage.inspect_rom(data, env, changed_table)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid ROM accepted")
    other = next(key for key in stage.TARGETS if key != env)
    try:
        stage.inspect_rom(marker, other, table)
    except ValueError:
        pass
    else:
        raise AssertionError("wrong target accepted")
print("release ROM: both target identities, ambiguous marker and slot budgets passed")
