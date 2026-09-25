"""Bounded COM5 capture for Note4 bring-up evidence (no credentials sent)."""

import argparse
import time

import serial


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", default="COM5")
    parser.add_argument("--seconds", type=int, default=20)
    parser.add_argument("--command", default="status")
    parser.add_argument("--reset", action="store_true")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    port = serial.Serial()
    port.port = args.port
    port.baudrate = 115200
    port.timeout = 0.2
    port.dtr = False
    port.rts = False
    port.open()
    with port, open(args.out, "wb") as out:
        if args.reset:
            port.rts = True
            time.sleep(0.15)
            port.rts = False
        time.sleep(1)
        if args.command:
            port.write((args.command + "\n").encode("ascii"))
        end = time.monotonic() + args.seconds
        while time.monotonic() < end:
            chunk = port.read(2048)
            if chunk:
                out.write(chunk)


if __name__ == "__main__":
    main()
