#!/usr/bin/env python3
"""Synthetic SPI/JD9853 pixels and TE checks; no original ROM or SIM image."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
BASE, SPI = 0x20fc1000, 0x2000a200
GPIO = lambda pin: 0x200008c4 + 4 * pin


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def ppm(path):
    with path.open('rb') as file:
        expect(file.readline().strip() == b'P6', 'screendump is not RGB PPM')
        width, height = map(int, file.readline().split())
        expect(file.readline().strip() == b'255', 'PPM sample width')
        pixels = file.read()
    expect(len(pixels) == width * height * 3, 'truncated screendump')
    return width, height, lambda x, y: tuple(pixels[(y * width + x) * 3:(y * width + x) * 3 + 3])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in existing directory')
    token = 'ap01-lcd-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    gdb, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, ideal_te_only=True,
                  icount='shift=0,align=off,sleep=off', checks=[], screenshots=[])
    process = gs = qs = qmp = None
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            deadline = time.monotonic() + 30
            with (output.parent / (token + '.log')).open('x') as log:
                process = subprocess.Popen(
                    [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on,clic-mock=on',
                     '-cpu', 'rv32,x-ap01-clic=on', '-kernel', str(image), '-S',
                     '-icount', 'shift=0,align=off,sleep=off',
                     '-display', 'none', '-serial', 'none', '-monitor', 'none',
                     '-gdb', f'unix:{gdb},server=on,wait=off',
                     '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                    stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                    preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def write(address, value, size=4):
                    expect(rsp.request(f'M{address:x},{size:x}:' + value.to_bytes(size, 'little').hex()) == 'OK', 'write rejected')

                def read(address, size=4):
                    return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},{size:x}')), 'little')

                def setreg(number, value):
                    expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'setreg')

                def run(words):
                    for index, word in enumerate(words + [0x13]):
                        write(BASE + index * 4, word)
                    end = BASE + len(words) * 4
                    setreg(32, BASE)
                    expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('T', 'S')), 'continue stop')
                        raw = bytes.fromhex(rsp.request('g'))
                        expect(int.from_bytes(raw[128:132], 'little') == end, 'unexpected guest stop')
                    finally:
                        rsp.request(f'z0,{end:x},4')

                def reject(request):
                    try:
                        rsp.request(request)
                    except RuntimeError as exc:
                        expect(str(exc).startswith('RSP '), 'wrong rejection')
                    else:
                        raise AssertionError('unsupported operation accepted: ' + request)

                def state():
                    return json.loads(qmp.command('qom-get', {'path': '/machine', 'property': 'lcd-state'}))

                def line(pin, high):
                    write(GPIO(pin), 0x40 | (1 << (25 if high else 26)))

                def command(value, parameters=()):
                    write(SPI, 0x81)
                    line(30, False)
                    write(SPI + 0x88, value, 1)
                    line(30, True)
                    for byte in parameters:
                        write(SPI + 0x88, byte, 1)

                def window(x0, x1, y0, y1):
                    command(0x2a, [x0 >> 8, x0 & 255, x1 >> 8, x1 & 255])
                    command(0x2b, [y0 >> 8, y0 & 255, y1 >> 8, y1 & 255])

                def screenshot(name):
                    path = output.parent / (token + '-' + name + '.ppm')
                    qmp.command('screendump', {'filename': str(path)})
                    report['screenshots'].append(str(path))
                    width, height, pixel = ppm(path)
                    expect((width, height) == (320, 240), 'explicit clockwise output dimensions')
                    return pixel

                def check(name, fn):
                    report['active_check'] = name
                    fn()
                    report['checks'].append(dict(name=name, passed=True))

                def pixels():
                    expect(read(SPI + 0x84) & 0x3f == 32, 'empty TX slots must be32')
                    line(32, True)
                    line(28, False)
                    command(0x01)
                    command(0x3a, [5])
                    command(0x36, [0])
                    window(1, 2, 3, 4)
                    command(0x11)
                    command(0x29)
                    command(0x2c)
                    # Frame16 with MSB-byte-first sends actual RGB565 pairs.
                    write(SPI, 0x85)
                    for rgb in (0xf800, 0x07e0, 0x001f, 0xffff):
                        write(SPI + 0x88, rgb, 2)
                    pixel = screenshot('native-gram-clockwise')
                    for x, y, color in ((316, 1, (255, 0, 0)), (316, 2, (0, 255, 0)),
                                        (315, 1, (0, 0, 255)), (315, 2, (255, 255, 255))):
                        expect(pixel(x, y) == color, f'RGB565/native GRAM rotation mismatch{x,y}')
                    expect(pixel(0, 0) == (0, 0, 0), 'unwritten GRAM must stay black')
                    command(0x36, [0xe8])  # MV/MX/MY plus BGR, independent of output rotation.
                    window(5, 6, 7, 7)
                    command(0x2c)
                    write(SPI, 0x85)
                    for rgb in (0xf800, 0x07e0):
                        write(SPI + 0x88, rgb, 2)
                    pixel = screenshot('madctl-swap-mirror-bgr')
                    expect(pixel(5, 232) == (0, 0, 255) and pixel(6, 232) == (0, 255, 0),
                           'MADCTL native swap/mirror/BGR failed')
                    snapshot = state()
                    expect(snapshot['pixel_writes'] == 6 and snapshot['display_on'] and snapshot['sleep_out'],
                           'panel transaction counters/state')
                    report['draw_state'] = snapshot
                check('actual SPI pixels, RGB565, native MADCTL and explicit clockwise output', pixels)

                def full_duplex_samples():
                    write(SPI + 0x80, 8)
                    write(SPI, 0x81)
                    line(30, False)
                    setreg(10, SPI)
                    setreg(11, 0x11)
                    setreg(15, 1)
                    # Native equivalent of original62fc223a: TX one frame,
                    # poll RX_CNT[13:8]>=frame bytes, then consume RDATA.
                    displacement = (-12) & 0x1fff
                    branch = (((displacement >> 12) & 1) << 31 |
                              ((displacement >> 5) & 63) << 25 | 15 << 20 |
                              14 << 15 | 6 << 12 | ((displacement >> 1) & 15) << 8 |
                              ((displacement >> 11) & 1) << 7 | 0x63)
                    run([0x08b52423, 0x08452703, 0x00875713, 0x03f77713,
                         branch, 0x08c52603])
                    registers = bytes.fromhex(rsp.request('g'))
                    expect(int.from_bytes(registers[48:52], 'little') == 0xff,
                           'native full-duplex send did not read synthetic pull-high sample')
                    expect((read(SPI + 0x84) >> 8) & 63 == 0, 'native RDATA did not consume one frame')
                    for config, word, frame in ((0x85, 0x1129, 2), (0x8d, 0x11291129, 4)):
                        write(SPI, config)
                        write(SPI + 0x88, word, 4)
                        expect((read(SPI + 0x84) >> 8) & 63 == frame, 'RX byte count differs from real TX frame')
                        expect(read(SPI + 0x8c) == (1 << (frame * 8)) - 1, 'RX sampled word width')
                        expect((read(SPI + 0x84) >> 8) & 63 == 0, 'RX frame was not consumed')
                    write(SPI, 0x81)
                    for _ in range(40):
                        write(SPI + 0x88, 0x11, 1)
                    expect((read(SPI + 0x84) >> 8) & 63 == 32, 'RX FIFO exceeded bounded capacity')
                    expect(read(SPI + 0x80) & 0x40, 'RX overflow not recorded')
                    for _ in range(32):
                        expect(read(SPI + 0x8c) == 0xff, 'FIFO sample changed')
                    reject('m2000a28c,4')
                    expect(read(SPI + 0x80) & 0x80, 'empty RX read did not record underflow')
                    write(SPI + 0x80, 8)
                    expect(not read(SPI + 0x80) & 0xc0, 'RX FIFO clear did not clear sticky errors')
                    expect((read(SPI + 0x84) >> 8) & 63 == 0, 'FIFO clear fabricated RX readiness')
                check('native SPI RX poll consumes only real clocked pull-high samples with bounded FIFO', full_duplex_samples)

                def errors():
                    write(SPI + 0x80, 8)
                    reject('m2000a28c,4')  # No clocked RX sample exists.
                    reject('m2000a224,4')  # Unknown register.
                    reject('M2000a280,4:02000000')  # RX enable unsupported.
                    reject('M2000a200,4:81010000')  # RX-ignore window unsupported.
                    command(0x28)
                    expect(screenshot('display-off')(5, 232) == (0, 0, 0), 'display-off must blank output')
                    command(0x29)
                    before = state()['tx_bytes']
                    line(28, True)
                    reject('M2000a288,1:01')
                    expect(state()['tx_bytes'] == before, 'CS-high rejection mutated byte count')
                    line(28, False)
                    line(30, False)
                    reject('M2000a288,1:ff')
                check('SPI RX/unknown/CS rejection and display blanking', errors)

                def te():
                    line(32, True)
                    line(28, False)
                    command(0x11)
                    command(0x29)
                    command(0x35, [0])
                    te_gpio = GPIO(23)
                    report['te_modes'] = []
                    write(0x20009014, 0xc000001f)
                    write(0x20009014, 0x8000001f)
                    rising_times = []
                    for _ in range(2):
                        write(te_gpio, 1 | 1 << 16 | 1 << 20)
                        setreg(5, te_gpio)
                        setreg(7, 1 << 21)
                        run([0x0002a303, 0x00737333, 0xfe030ce3])
                        rising_times.append(read(0xe000bff8))
                    period_us = rising_times[1] - rising_times[0]
                    report['te_rising_period_virtual_us'] = period_us
                    expect(16000 <= period_us <= 18000, 'TE rising period is not ideal60Hz')
                    # Actual guest waits on STAT21; no host-synthesized edge
                    # or fabricated image is involved. TE clock runs in QEMU.
                    for mode in (1, 9, 0, 8, 4, 2, 10, 3, 11):
                        config = 1 | mode << 16
                        write(te_gpio, config | 1 << 20)
                        setreg(5, te_gpio)
                        setreg(7, 1 << 21)
                        run([0x0002a303, 0x00737333, 0xfe030ce3])
                        value = read(te_gpio)
                        expect(value & (1 << 21), f'TE mode{mode} did not latch status')
                        if mode in (1, 9, 3, 11):
                            expect(value & (1 << 28), f'TE mode{mode} expected high')
                        elif mode in (0, 8, 2, 10):
                            expect(not value & (1 << 28), f'TE mode{mode} expected low')
                        expect(read(0xe08010f0, 1) & 1, 'TE GPIO did not raise raw CLIC60')
                        write(te_gpio, config | 1 << 22)
                        expect(not read(0xe08010f0, 1) & 1, 'GPIO mask did not gate CLIC60')
                        report['te_modes'].append(dict(mode=mode, input_high=bool(value & 1 << 28),
                                                       status=True, clic60=True))
                    command(0x34)
                    expect(not state()['te'], 'TE disable command not committed')
                    # Clear in edge mode and execute until the actual virtual
                    # MTIMER advances20ms, longer than a complete TE period.
                    write(te_gpio, 1 | 4 << 16 | 1 << 20)
                    frozen_level = read(te_gpio) & (1 << 28)
                    write(0x20009014, 0xc000001f)
                    write(0x20009014, 0x8000001f)
                    setreg(5, 0xe000bff8)
                    setreg(7, 20000)
                    run([0x0002a303, 0xfe736ee3])
                    report['te_disabled_virtual_us'] = read(0xe000bff8)
                    expect(not read(te_gpio) & (1 << 21), 'disabled TE still generated an edge')
                    expect(read(te_gpio) & (1 << 28) == frozen_level, 'disabled TE input changed')
                    line(32, False)
                    expect(not state()['display_on'] and not state()['sleep_out'], 'active-low panel reset did not clear state')

                bank_out = (0x20000ae4, 0x20000ae8)
                bank_set = (0x20000aec, 0x20000af0)
                bank_clear = (0x20000af4, 0x20000af8)

                def native_store(address, value):
                    setreg(5, address)
                    setreg(6, value)
                    run([0x0062a023])  # sw t1,0(t0), real guest MMIO access.

                def bank_aliases():
                    native_store(bank_out[0], 0xffffffff)
                    native_store(bank_out[1], 7)
                    expect(read(bank_out[0]) == 0xffffffff and read(bank_out[1]) == 7,
                           'bank output width/masks')
                    for pin in range(35):
                        expect(read(GPIO(pin)) & (1 << 24), f'bank output not mirrored in pad{pin}')
                    native_store(bank_out[0], 0)
                    native_store(bank_out[1], 0)
                    for pin in range(35):
                        expect(not read(GPIO(pin)) & (1 << 24), f'bank output clear not mirrored pad{pin}')
                    native_store(bank_set[0], (1 << 5) | (1 << 31))
                    native_store(bank_set[1], (1 << 0) | (1 << 2))
                    expect(read(bank_out[0]) == (1 << 5) | (1 << 31), 'SET bank0')
                    expect(read(bank_out[1]) == 5, 'SET bank1')
                    line(5, False)
                    line(31, False)
                    line(33, True)
                    expect(read(bank_out[0]) == 0, 'pad clear did not update bank0')
                    expect(read(bank_out[1]) == 7, 'pad set did not update bank1')
                    native_store(bank_clear[1], 2)
                    expect(not read(GPIO(33)) & (1 << 24), 'bank CLEAR did not update pad')
                    before = (read(bank_out[0]), read(bank_out[1]), state())
                    for address in bank_set + bank_clear:
                        native_store(address, 0)
                    expect((read(bank_out[0]), read(bank_out[1]), state()) == before,
                           'zero SET/CLEAR mutated output/panel state')
                check('native GPIO banks and per-pad latches agree, zero strobes preserve state', bank_aliases)

                def bank_protection():
                    before = read(bank_out[1])
                    for address in (bank_out[1], bank_set[1], bank_clear[1]):
                        reject(f'M{address:x},4:08000000')
                        expect(read(bank_out[1]) == before, 'reserved bank1 bit mutated outputs')
                    for address in bank_out + bank_set + bank_clear:
                        reject(f'M{address:x},1:01')
                        reject(f'M{address:x},2:0100')
                    reject(f'm{bank_out[0]:x},1')
                    reject(f'm{bank_out[1]:x},2')
                    expect(read(bank_out[1]) == before, 'invalid width changed bank1')
                check('GPIO bank reserved bits and unsupported widths reject atomically', bank_protection)

                def bank_panel_lines():
                    native_store(bank_set[1], 1)  # RESET32 high.
                    native_store(bank_clear[0], (1 << 28) | (1 << 30))
                    write(SPI, 0x81)
                    # DC30 via bank aliases: real commands, no per-pad helper.
                    for value in (0x11, 0x29):
                        native_store(bank_clear[0], 1 << 30)
                        write(SPI + 0x88, value, 1)
                    expect(state()['display_on'] and state()['sleep_out'], 'bank DC/CS did not deliver commands')
                    native_store(bank_set[0], 1 << 28)
                    before = state()['tx_bytes']
                    reject('M2000a288,1:28')
                    expect(state()['tx_bytes'] == before and state()['display_on'], 'bank CS-high changed panel')
                    native_store(bank_clear[0], 1 << 28)
                    native_store(bank_clear[1], 1)
                    expect(not state()['display_on'] and not state()['sleep_out'], 'bank reset low did not reset real panel')
                    native_store(bank_set[1], 1)
                    expect(not state()['display_on'] and not state()['sleep_out'], 'reset deassertion invented display setup')
                check('native bank SET/CLEAR drives actual SPI CS DC and active-low panel reset', bank_panel_lines)
                check('ideal TE clock, GPIO23 modes/mask/clear and raw IRQ60', te)
            report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
        finally:
            if qmp is not None:
                qmp.file.close()
            for sock in (gs, qs):
                if sock is not None:
                    sock.close()
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=2)
                for path in (gdb, qmp_path):
                    if path.exists():
                        path.unlink()
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
