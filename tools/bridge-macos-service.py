#!/usr/bin/env python3
"""On-demand GUI LaunchAgent; no login registration, secrets, or shell eval."""
import argparse
import os
import pathlib
import plistlib
import re
import subprocess
import sys


def specification(app, name, environment, starting=True):
    if name and not re.fullmatch(r"[a-z][a-z0-9-]{0,26}", name):
        raise ValueError("invalid instance name")
    if name == "default":
        raise ValueError("default is reserved")
    root = pathlib.Path.home() / "Library/Application Support/dev.codexstatus.bridge"
    data = pathlib.Path(environment.get("CODEX_STATUS_DATA") or
                        (root / "instances" / name / "data" if name else root / "data"))
    label = "dev.codexstatus.bridge.managed" + ("." + name if name else "")
    exe = app.resolve() / "Contents/MacOS/bridge-app"
    # Configuration tokens remain in private data; never copy them to a plist.
    allowed = {key: value for key, value in environment.items() if key in {
        "CODEX_STATUS_DATA", "CODEX_STATUS_PORT", "CODEX_STATUS_MCP_PORT",
        "CODEX_STATUS_ICON_SHAPE", "CODEX_STATUS_DEVICE_MAC", "CODEX_STATUS_DEVICE_IP",
        "CODEX_STATUS_BRIDGE_NAME", "CODEX_STATUS_CODEX"}}
    if name:
        if starting and (not allowed.get("CODEX_STATUS_PORT") or not allowed.get("CODEX_STATUS_MCP_PORT")):
            raise ValueError("named instance requires explicit HTTP and MCP ports")
        allowed["CODEX_STATUS_INSTANCE"] = name
    http = int(allowed.get("CODEX_STATUS_PORT", "8765"))
    mcp = int(allowed.get("CODEX_STATUS_MCP_PORT", "8766"))
    if http == mcp or not (1024 <= http <= 65535 and 1024 <= mcp <= 65535):
        raise ValueError("invalid/distinct ports required")
    document = {"Label": label, "ProgramArguments": [str(exe), "--managed-supervisor"],
                "RunAtLoad": False, "KeepAlive": {"SuccessfulExit": False},
                "ThrottleInterval": 10, "ProcessType": "Interactive", "EnvironmentVariables": allowed}
    return root / (label + ".plist"), data, document


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["start", "stop", "status"])
    parser.add_argument("--app", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[1] / "artifacts/bridge-macos/Codex Status Bridge.app")
    parser.add_argument("--instance", default=os.environ.get("CODEX_STATUS_INSTANCE", ""))
    args = parser.parse_args()
    path, data, document = specification(args.app, args.instance, os.environ, args.action == "start")
    target = f"gui/{os.getuid()}/{document['Label']}"
    status = subprocess.run(["/bin/launchctl", "print", target], capture_output=True, text=True)
    if args.action == "status":
        print("loaded" if status.returncode == 0 else "not loaded")
        if status.returncode == 0:
            for line in status.stdout.splitlines():
                if line.strip().startswith(("state =", "pid =", "last exit code =")):
                    print(line.strip())
        return
    if args.action == "stop":
        if status.returncode == 0:
            subprocess.run(["/bin/launchctl", "bootout", target], check=True)
        print("managed instance stopped; data preserved")
        return
    if not pathlib.Path(document["ProgramArguments"][0]).is_file():
        raise ValueError("build the app before starting")
    if status.returncode == 0:
        # Never replace or kill an already loaded job to reuse another path.
        existing = plistlib.loads(path.read_bytes()) if path.is_file() else {}
        if existing != document:
            raise ValueError("loaded job uses different configuration; stop this managed instance first")
    else:
        # Run this creation step with elevated tool permissions per AGENTS.md.
        path.parent.mkdir(parents=True, exist_ok=True)
        (data / "logs").mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix(".plist.tmp")
        temporary.write_bytes(plistlib.dumps(document))
        temporary.chmod(0o600)
        temporary.replace(path)
        subprocess.run(["/bin/launchctl", "bootstrap", f"gui/{os.getuid()}", str(path)], check=True)
    subprocess.run(["/bin/launchctl", "kickstart", "-p", target], check=True)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
