"""Explicit finite empty Wi-Fi scan API model; no APs, association or radio."""
APIS = (0xa00761b4, 0xa00762d4, 0xa0076370)


class EmptyScan:
    def __init__(self, read, report):
        self.read, self.report = read, report
        self.transaction = None
        report['empty_scan'] = dict(starts=0, pending=0, completed=0, results=0,
            latency_ms=20, result_policy='original driver empty list -ENOENT; head remains NULL')

    def handle(self, pc, args):
        fd, name = args[:2]
        if not 0 <= fd < 0x4000 or self.read(name, 6) != b'wlan0\0':
            return -22
        now = int.from_bytes(self.read(0x62fc6c64, 4), 'little')
        counts = self.report['empty_scan']
        if pc == APIS[0]:
            if self.transaction and not self.transaction['consumed']:
                return -16
            if counts['starts'] >= 64:
                raise RuntimeError('empty scan request budget exceeded')
            self.transaction = dict(fd=fd, started=now, consumed=False, ready=False)
            counts['starts'] += 1
            return 0
        current = self.transaction
        if not current or current['fd'] != fd or current['consumed']:
            return -22
        if pc == APIS[1]:
            if ((now - current['started']) & 0xffffffff) < 20:
                counts['pending'] += 1
                return 1
            current['ready'] = True
            counts['completed'] += 1
            return 0
        if pc != APIS[2]:
            raise ValueError('unsupported scan API')
        if not current['ready']:
            return -11
        if len(args) < 3 or not args[2] or self.read(args[2], 4) != bytes(4):
            return -22
        current['consumed'] = True
        counts['results'] += 1
        # Actual original driver has no AP entries: -ENOENT, never fabricate
        # scan records. The original caller tolerates this after completion.
        return -2
