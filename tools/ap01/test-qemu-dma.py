#!/usr/bin/env python3
"""Bounded synthetic DMA0 integration; no original ROM or physical devices."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
DMA, RAM, CLIC31 = 0x2000c000, 0x20fc4000, 0xe080107c


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in existing directory')
    token = 'ap01-dma-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    image.write_bytes(bytes(4096) + bytes.fromhex('13000000'))
    log_path = output.parent / (token + '.log')
    gdb, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, clic_pending_verified=False,
                  clic_dispatch_verified=False, checks=[], fixture=str(image),
                  qemu=str(qemu), qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest())
    process = gs = qs = qm = None
    deadline = time.monotonic() + 30
    with output.open('x', encoding='utf-8') as result:
        try:
            command = [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on,clic-mock=on',
                       '-cpu', 'rv32,x-ap01-clic=on', '-kernel', str(image), '-S',
                       '-display', 'none', '-serial', 'none', '-monitor', 'none',
                       '-gdb', f'unix:{gdb},server=on,wait=off',
                       '-qmp', f'unix:{qmp_path},server=on,wait=off']
            report.update(command=command, log=str(log_path))
            with log_path.open('x') as log:
                process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                           stdout=log, stderr=log, preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qm = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def put(address, data):
                    expect(rsp.request(f'M{address:x},{len(data):x}:' + data.hex()) == 'OK',
                           f'write rejected at {address:x}')

                def write(address, value):
                    put(address, value.to_bytes(4, 'little'))

                def get(address, count=4):
                    return bytes.fromhex(rsp.request(f'm{address:x},{count:x}'))

                def read(address):
                    return int.from_bytes(get(address), 'little')

                def rejected(address, value=None):
                    request = (f'm{address:x},4' if value is None else
                               f'M{address:x},4:' + value.to_bytes(4, 'little').hex())
                    try:
                        reply = rsp.request(request)
                    except RuntimeError as exc:
                        if str(exc).startswith('RSP '):
                            return
                        raise
                    expect(reply.startswith('E'), f'access unexpectedly accepted: {request}')

                def ctrl(count, sw=0, dw=0, si=True, di=True, irq=True):
                    return (count | sw << 18 | dw << 21 | int(si) << 26 |
                            int(di) << 27 | int(irq) << 31)

                def configure(channel, src, dst, control, lli=0):
                    base = DMA + 0x100 + channel * 256
                    for off, value in ((0x10, 0), (0, src), (4, dst), (8, lli), (12, control)):
                        write(base + off, value)
                    return base

                def check(name, function):
                    report['active_check'] = name
                    qm.command('system_reset')
                    function()
                    report['checks'].append(dict(name=name, passed=True))

                def widths():
                    payload = bytes(range(32))
                    put(RAM, payload)
                    write(DMA + 0x30, 1)
                    for sw, dw in ((0, 0), (0, 2), (2, 0), (1, 2), (2, 1)):
                        put(RAM + 0x100, bytes(32))
                        base = configure(0, RAM, RAM + 0x100, ctrl(32 >> sw, sw, dw))
                        write(base + 0x10, 0xc001)
                        expect(get(RAM + 0x100, 32) == payload, f'width conversion {sw}/{dw}')
                        expect(read(base) == RAM + 32 and read(base + 4) == RAM + 0x120,
                               'source/destination advancement')
                        expect(read(base + 0x10) & 1 == 0 and read(base + 12) & 0xfff == 0,
                               'completion did not clear enable/count')
                        write(DMA + 8, 1)
                    rejected(DMA + 0x20, 1)
                check('M2M byte/halfword/word packing and software-request rejection', widths)

                def chain_irq():
                    put(RAM, b'first---second!!')
                    put(RAM + 0x300, struct.pack('<4I', RAM + 8, RAM + 0x108, 0, ctrl(8)))
                    base = configure(3, RAM, RAM + 0x100, ctrl(8), RAM + 0x300)
                    write(DMA + 0x30, 1)
                    write(CLIC31, 0x100)
                    write(base + 0x10, 0xc001)
                    expect(get(RAM + 0x100, 16) == b'first---second!!', 'LLI bytes differ')
                    expect(read(DMA + 0x14) == 8 and read(DMA + 4) == 8, 'raw/masked TC')
                    expect(read(DMA) == 8 and read(CLIC31) & 1, 'IRQ31 not pending')
                    write(base + 0x10, 0x4000)
                    expect(read(DMA + 4) == 0 and read(DMA + 0x14) == 8, 'TC masking')
                    write(base + 0x10, 0xc000)
                    expect(read(CLIC31) & 1, 'TC unmask lost pending')
                    write(DMA + 8, 8)
                    expect(read(DMA + 0x14) == 0 and read(CLIC31) & 1 == 0, 'TC W1C/IRQ clear')
                    report['clic_pending_verified'] = True
                check('two LLIs, TC masks/W1C and raw CLIC IRQ31', chain_irq)

                def errors():
                    write(DMA + 0x30, 1)
                    put(RAM, b'abcd')
                    for src, dst, control, lli in (
                        (0x2000a000, RAM + 0x100, ctrl(4), 0),
                        (RAM, 0x9ffff000, ctrl(4), 0),
                        (0x20fc0000 + 0x78000 - 2, RAM + 0x100, ctrl(4), 0),
                        (RAM, RAM + 0x100, ctrl(4, 3, 0), 0),
                        (RAM, RAM + 0x100, ctrl(4), RAM + 0x301),
                    ):
                        base = configure(1, src, dst, control, lli)
                        rejected(base + 0x10, 0xc001)
                        expect(read(DMA + 0x18) & 2 and read(DMA + 0xc) & 2, 'ERR missing')
                        expect(read(base + 0x10) & 1 == 0, 'ERR channel still enabled')
                        write(DMA + 0x10, 2)
                        write(DMA + 8, 2)
                        expect(read(DMA + 0x18) == 0 and read(DMA + 0xc) == 0, 'ERR W1C')
                    put(RAM + 0x300, struct.pack('<4I', RAM, RAM + 0x100,
                                               RAM + 0x300, ctrl(4, irq=False)))
                    base = configure(1, RAM, RAM + 0x100, ctrl(4, irq=False), RAM + 0x300)
                    rejected(base + 0x10, 0xc001)
                    expect(read(DMA + 0x18) == 2 and read(DMA + 0x14) == 0, 'cycle falsely completed')
                check('MMIO/XIP-write/range/width/LLI rejection and bounded cycle ERR', errors)

                def spi_dma():
                    def gpio(pin, high):
                        write(0x200008c4 + pin * 4, 0xb00 | 64 | int(high) << 24)
                    gpio(32, True)
                    gpio(28, False)
                    def command(byte, params=b''):
                        write(0x2000a200, 1)
                        gpio(30, False)
                        write(0x2000a288, byte)
                        gpio(30, True)
                        for param in params:
                            write(0x2000a288, param)
                    command(0x11)
                    command(0x29)
                    command(0x3a, b'\x55')
                    command(0x2c)
                    state = lambda: json.loads(qm.command('qom-get', {'path': '/machine', 'property': 'lcd-state'}))
                    before = state()
                    write(0x2000a200, 0x85)  # frame16, MSB byte first
                    write(0x2000a280, 1)  # TX DMA gate
                    put(RAM, bytes.fromhex('00f8e0071f00'))
                    base = configure(3, RAM, 0x2000a288, ctrl(3, 1, 1, di=False))
                    write(DMA + 0x30, 1)
                    write(base + 0x10, 0xc001 | 11 << 6 | 1 << 11)
                    after = state()
                    expect(after['tx_bytes'] - before['tx_bytes'] == 6, 'SPI16 DMA byte count')
                    expect(after['pixel_writes'] - before['pixel_writes'] == 3, 'SPI16 DMA pixel count')
                    expect(read(DMA + 4) == 8, 'SPI DMA did not complete')
                    write(DMA + 8, 8)
                    write(0x2000a280, 0)
                    base = configure(3, RAM, 0x2000a288, ctrl(3, 1, 1, di=False))
                    rejected(base + 0x10, 0xc001 | 11 << 6 | 1 << 11)
                    expect(state()['tx_bytes'] == after['tx_bytes'], 'disabled SPI DMA consumed bytes')
                    report['lcd_before'], report['lcd_after'] = before, after
                check('SPI16 DMA exact byte/pixel counters, TX gate failure', spi_dma)

                def uart_wait():
                    write(0x2000a104, 1)
                    write(0x2000a180, 2)
                    expect(read(0x2000a180) & 2, 'RX DMA gate did not latch')
                    rejected(0x2000a180, 1)
                    put(RAM + 0x100, b'unchanged')
                    base = configure(2, 0x2000a18c, RAM + 0x100, ctrl(8, si=False))
                    write(DMA + 0x30, 1)
                    write(base + 0x10, 0xc001 | 2 << 1 | 2 << 11)
                    expect(read(base + 0x10) & 1 and read(DMA + 0x1c) & 4, 'UART RX not waiting')
                    expect(read(DMA + 0x14) == 0 and read(DMA + 0x18) == 0, 'UART RX false TC/ERR')
                    expect(get(RAM + 0x100, 9) == b'unchanged', 'UART RX wrote synthetic bytes')
                    qm.command('system_reset')
                    expect(read(DMA + 0x1c) == 0 and read(DMA + 0x30) == 0, 'DMA reset')
                check('UART1 RX DMA waits without input; reset cancels channel', uart_wait)

                def native_write(address, value):
                    program = RAM + 0x1000
                    def li(reg, value):
                        hi = (value + 0x800) & 0xfffff000
                        return [hi | reg << 7 | 0x37,
                                ((value - hi) & 0xfff) << 20 | reg << 15 | reg << 7 | 0x13]
                    # Real RV32 store/readback; trap goes to a separate terminal
                    # so a rejected config cannot masquerade as a returned write.
                    end = program + 7 * 4
                    words = li(31,end)+[0x305f9073]+li(5,address)+li(6,value)+[0x0062a023,0x0002a383]
                    end = program + len(words) * 4
                    words[:2] = li(31,end)
                    put(program,b''.join(struct.pack('<I',w) for w in words)+bytes.fromhex('13000000'))
                    expect(rsp.request('P7=efbeadde')=='OK','native load sentinel')
                    expect(rsp.request('P20='+program.to_bytes(4,'little').hex())=='OK','native PC')
                    expect(rsp.request(f'Z0,{end:x},4')=='OK','native breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('S','T')),'native stop')
                        regs=struct.unpack('<33I',bytes.fromhex(rsp.request('g'))[:132])
                        expect(regs[32]==end and regs[7]==read(address),'native DMA store/load did not complete')
                    finally:
                        rsp.request(f'z0,{end:x},4')

                def halt_resume():
                    payload=b'haltDATA'
                    put(RAM,payload);put(RAM+0x100,bytes(8))
                    base=configure(0,RAM,RAM+0x100,ctrl(8))
                    halted=0x4c001
                    native_write(base+0x10,halted)
                    native_write(DMA+0x30,1)
                    native_write(base+0x10,halted)
                    expect(get(RAM+0x100,8)==bytes(8),'halted channel transferred bytes')
                    expect(read(base)==RAM and read(base+4)==RAM+0x100 and read(base+12)&0xfff==8,'halt changed progress')
                    expect(read(base+0x10)==halted and read(DMA+0x1c)&1,'halt discarded enable')
                    expect(read(DMA+0x14)==0 and read(DMA+0x18)==0 and not read(CLIC31)&1,'halt fabricated TC/ERR/IRQ')
                    rejected(base,RAM+4) # enabled/halted is not stopped/configurable
                    native_write(base+0x10,halted&~0x40000)
                    expect(get(RAM+0x100,8)==payload and read(DMA+0x14)==1,'resume did not complete saved transfer')
                    expect(not read(base+0x10)&1 and read(DMA+0x18)==0,'resume retained E or fabricated ERR')
                    write(DMA+8,1)
                    # Observed UART stop pattern: set H, clear E, then reconfigure.
                    base=configure(0,RAM,RAM+0x100,ctrl(8))
                    native_write(base+0x10,halted)
                    native_write(base+0x10,halted&~1)
                    expect(not read(base+0x10)&0x20000,'synchronous halt falsely reports active beat')
                    write(base+12,ctrl(4))
                    expect(read(base+12)&0xfff==4 and read(DMA+0x14)==0,'stop/config sequence failed')
                    native_write(base+0x10,0xc001)
                    expect(read(DMA+0x14)==1 and read(DMA+0x18)==0,'restart after stop failed')
                    write(DMA+8,1)
                    write(0x2000a104,1);write(0x2000a180,2)
                    put(RAM+0x100,b'unchanged')
                    uart=configure(2,0x2000a18c,RAM+0x100,ctrl(8,si=False))
                    uart_config=0xc001 | 2<<1 | 2<<11
                    native_write(uart+0x10,uart_config)
                    native_write(uart+0x10,uart_config|0x40000)
                    native_write(uart+0x10,uart_config)
                    expect(read(uart+0x10)&1 and read(DMA+0x14)==0 and read(DMA+0x18)==0,'halt/resume fabricated RX completion/error')
                    expect(get(RAM+0x100,9)==b'unchanged','halt/resume fabricated RX payload')
                    native_write(uart+0x10,(uart_config|0x40000)&~1)
                    write(uart+12,ctrl(4,si=False))
                    base=configure(1,RAM,RAM+0x100,ctrl(4))
                    rejected(base+0x10,0x5c001)
                    expect(read(DMA+0x18)==2,'unsupported lock silently treated as halt')
                check('native halt retains state without TC/ERR, resume/stop/reconfigure and lock rejection',halt_resume)

                report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
            raise
        finally:
            if qm:
                try:
                    qm.command('quit')
                except Exception:
                    pass
            for sock in (gs, qs):
                if sock:
                    sock.close()
            if process:
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)
            for path in (gdb, qmp_path):
                path.unlink(missing_ok=True)
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
