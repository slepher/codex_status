#!/usr/bin/env python3
"""Apply or safely upgrade the opt-in AP01 CPU patch on the pinned checkout."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
COMMIT = 'f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3'

# Exact prior combined patch (through SUB64/IRQ79), before MAXW. Only these
# two files change in this upgrade; all board/header and other edits survive.
MAXW_PREVIOUS = {
    'target/riscv/insn32.decode': '36b7a5e4b8ab6301c301310aad081b449baa4923827bd169a6d43c866ae9d72a',
    'target/riscv/insn_trans/trans_xthead.c.inc': '63e51c92799ce7772e56a5ec5790acb93701dd7825f1cc913df34294c9abf7c7',
}
# Verified MAXW version immediately preceding SWAP8. Upgrade only owned files.
SWAP8_PREVIOUS = {
    'target/riscv/insn32.decode': 'bde9b0bacf46b3ec599b66d3c92f304add7a175bc81e7d9437e17c31ce2835d8',
    'target/riscv/insn_trans/trans_xthead.c.inc': '21cf3b721f8ebe0238c53dc8ec794612c7324920ba1f3f001a60fefe473d8d70',
}
UMAR64_PREVIOUS = {
    'target/riscv/insn32.decode': '3d177090bb7eee485317b20f567ea8e17754bd001969ee934d4d5547db3cdf6a',
    'target/riscv/insn_trans/trans_xthead.c.inc': 'cc4bd89103d02423e681e59ceec07ec2865cb796452b0a4026cb1588ba97e2be',
}



def maxw_upgrade(patch_text, originals):
    """Prepare a known prior-version upgrade; refuse unknown source changes."""
    hashes = {name: hashlib.sha256(content).hexdigest() for name, content in originals.items()}
    if hashes == MAXW_PREVIOUS:
        instructions = ('maxw', 'swap8', 'umar64')
    elif hashes == SWAP8_PREVIOUS:
        instructions = ('swap8', 'umar64')
    elif hashes == UMAR64_PREVIOUS:
        instructions = ('umar64',)
    else:
        raise ValueError('owned CPU files do not match a verified prior patch version')
    additions = ''.join(line[1:] for line in patch_text.splitlines(keepends=True)
                        if line.startswith('+') and not line.startswith('+++'))
    result = originals.copy()
    for instruction in instructions:
        start = additions.index('/* ' + instruction.upper() + ' ')
        implementation = additions[start:additions.index('\n}\n\n', start) + 4]
        decoder = next(line for line in additions.splitlines(keepends=True)
                       if line.startswith('ap01_' + instruction + ' '))
        for name, original in result.items():
            text = original.decode('utf-8')
            if name.endswith('insn32.decode'):
                previous = {'maxw': 'mulr64', 'swap8': 'maxw', 'umar64': 'swap8'}[instruction]
                anchor = next(line for line in text.splitlines(keepends=True)
                              if line.startswith('ap01_' + previous + ' '))
                replacement = anchor + decoder
            else:
                anchor = '#define REQUIRE_XTHEADBA(ctx)'
                replacement = implementation + anchor
            if text.count(anchor) != 1:
                raise ValueError(f'{name}: upgrade anchor is not unique')
            result[name] = text.replace(anchor, replacement, 1).encode('utf-8')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve()
    patches = [Path(__file__).resolve().with_name('qemu-ap01-cpu.patch')]

    def git(*arguments):
        return subprocess.run(['git', '-C', str(source), *arguments],
                              capture_output=True, text=True, timeout=30)

    head = git('rev-parse', 'HEAD')
    if head.returncode or head.stdout.strip() != COMMIT:
        parser.error(f'QEMU HEAD must be exactly {COMMIT}')
    for patch in patches:
        check = git('apply', '--check', str(patch))
        if check.returncode:
            reverse = git('apply', '--reverse', '--check', str(patch))
            if reverse.returncode == 0:
                print(f'{patch.name} already applied; no changes made')
                continue
            originals = {name: (source / name).read_bytes() for name in MAXW_PREVIOUS}
            try:
                replacements = maxw_upgrade(patch.read_text(encoding='utf-8'), originals)
            except (ValueError, StopIteration) as exc:
                parser.error(f'{patch.name} cannot safely upgrade: {exc}\n' + check.stderr)
            try:
                for name, content in replacements.items():
                    (source / name).write_bytes(content)
                upgraded = git('apply', '--reverse', '--check', str(patch))
                if upgraded.returncode:
                    raise ValueError(upgraded.stderr)
            except Exception:
                for name, content in originals.items():
                    (source / name).write_bytes(content)
                raise
            print(f'Upgraded verified prior-version {patch.name}; unrelated files unchanged')
            continue
        applied = git('apply', str(patch))
        if applied.returncode:
            parser.error('git apply failed:\n' + applied.stderr)
        print(f'Applied {patch.name}; extensions remain opt-in')


if __name__ == '__main__':
    main()
