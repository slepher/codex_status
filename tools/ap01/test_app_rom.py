#!/usr/bin/env python3
"""Bounded RV32 Unicorn checks for the AP01 0031 custom-App candidate."""

import sys
sys.dont_write_bytecode = True

import argparse
import hashlib
import json
import struct
import time
import zlib
from pathlib import Path

ROM_BASE = 0x9FFFF000
PACKAGE_VA = 0xA024C000
PATCH_PC = 0xA00B2732
TRAMPOLINE = 0xA001B0AC
CONTINUATION = 0xA00B2738
LV_OBJ_CREATE = 0xA00C1EC6
STOCK_MALLOC = 0xA007E1C4
STOCK_FREE = 0xA007C256
STACK_BASE = 0x63000000
STACK_SIZE = 64 * 1024
HEAP_BASE = 0xA8000000
HEAP_SIZE = 1024 * 1024
PACKAGE_HEADER = 112
MAX_INSTRUCTIONS = 250_000
MAX_SECONDS = 2.0
EXPECTED_BASE_SHA = bytes.fromhex(
    "8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb"
)


class TestFailure(RuntimeError):
    pass


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def put_u32(data, offset, value):
    struct.pack_into("<I", data, offset, value & 0xFFFFFFFF)


def align_page(value):
    return (value + 0xFFF) & ~0xFFF


def crc_package(package):
    put_u32(package, 64, zlib.crc32(package[PACKAGE_HEADER:]) & 0xFFFFFFFF)
    repair_header_crc(package)


def repair_header_crc(package):
    put_u32(package, 68, 0)
    put_u32(package, 68, zlib.crc32(package[:PACKAGE_HEADER]) & 0xFFFFFFFF)


def add_deps(path):
    if path is None:
        return
    dep = Path(path)
    if not dep.is_dir():
        raise TestFailure(f"--deps is not an existing directory: {dep}")
    sys.path.insert(0, str(dep))


def load_unicorn(deps):
    add_deps(deps)
    try:
        from unicorn import (UC_ARCH_RISCV, UC_HOOK_CODE, UC_MODE_RISCV32,
                             Uc)
        from unicorn.riscv_const import (UC_RISCV_REG_A0, UC_RISCV_REG_A1,
                                         UC_RISCV_REG_A2, UC_RISCV_REG_A3,
                                         UC_RISCV_REG_A4, UC_RISCV_REG_A5,
                                         UC_RISCV_REG_GP,
                                         UC_RISCV_REG_PC, UC_RISCV_REG_RA,
                                         UC_RISCV_REG_S0, UC_RISCV_REG_S1,
                                         UC_RISCV_REG_S10, UC_RISCV_REG_S11,
                                         UC_RISCV_REG_S2, UC_RISCV_REG_S3,
                                         UC_RISCV_REG_S4, UC_RISCV_REG_S5,
                                         UC_RISCV_REG_S6, UC_RISCV_REG_S7,
                                         UC_RISCV_REG_S8, UC_RISCV_REG_S9,
                                         UC_RISCV_REG_SP)
    except ImportError as exc:
        raise TestFailure("Unicorn 2.1.4 is required; pass its existing --deps directory") from exc
    return {
        "Uc": Uc,
        "arch": UC_ARCH_RISCV,
        "mode": UC_MODE_RISCV32,
        "hook_code": UC_HOOK_CODE,
        "PC": UC_RISCV_REG_PC,
        "RA": UC_RISCV_REG_RA,
        "SP": UC_RISCV_REG_SP,
        "GP": UC_RISCV_REG_GP,
        "A0": UC_RISCV_REG_A0,
        "A1": UC_RISCV_REG_A1,
        "A2": UC_RISCV_REG_A2,
        "A3": UC_RISCV_REG_A3,
        "A4": UC_RISCV_REG_A4,
        "A5": UC_RISCV_REG_A5,
        "s": [UC_RISCV_REG_S0, UC_RISCV_REG_S1, UC_RISCV_REG_S2,
              UC_RISCV_REG_S3, UC_RISCV_REG_S4, UC_RISCV_REG_S5,
              UC_RISCV_REG_S6, UC_RISCV_REG_S7, UC_RISCV_REG_S8,
              UC_RISCV_REG_S9, UC_RISCV_REG_S10, UC_RISCV_REG_S11],
    }


def as_address(value):
    if isinstance(value, str):
        return int(value, 0)
    if isinstance(value, int):
        return value
    raise TestFailure(f"invalid symbol address: {value!r}")


def symbol_tables(document):
    if not isinstance(document, dict):
        raise TestFailure("symbols.json must contain an object")
    tables = {}
    for name in ("base", "app"):
        value = document.get(name, {})
        if isinstance(value, dict) and isinstance(value.get("symbols"), dict):
            value = value["symbols"]
        if not isinstance(value, dict):
            raise TestFailure(f"symbols.json {name!r} table is missing")
        tables[name] = {str(k): as_address(v) for k, v in value.items()}
    return tables


def find_symbol(table, names, required=True):
    for name in names:
        if name in table:
            return table[name]
    if required:
        raise TestFailure(f"symbols.json is missing one of: {', '.join(names)}")
    return None


def artifact_files(directory):
    names = ("app.elf", "app.bin", "app.apx", "base.elf", "payload.bin",
             "ap01-1.0.2_0031-app-xip.bin", "manifest.json", "symbols.json")
    files = {}
    for name in names:
        path = directory / name
        if not path.is_file():
            raise TestFailure(f"missing build artifact: {path}")
        files[name] = path
    return files


def package_offset():
    offset = PACKAGE_VA - ROM_BASE
    if offset < 0:
        raise TestFailure("package VA precedes ROM XIP base")
    return offset


def package_length(image):
    offset = package_offset()
    require(len(image) >= offset + PACKAGE_HEADER, "XIP image is shorter than the App package header")
    length = u32(image, offset + 12)
    require(length >= PACKAGE_HEADER and offset + length <= len(image),
            "XIP App package length is outside the ROM image")
    return length


class RomRun:
    def __init__(self, unicorn, image, symbols, root=True, fail_alloc_at=None,
                 fail_init=False, fail_stop=False):
        self.uc = unicorn["Uc"](unicorn["arch"], unicorn["mode"])
        self.u = unicorn
        self.image = image
        self.symbols = symbols
        self.root = root
        self.fail_alloc_at = fail_alloc_at
        self.fail_init = fail_init
        self.fail_stop = fail_stop
        self.start_time = time.monotonic()
        self.instructions = 0
        self.error = None
        self.reached_continuation = False
        self.base_entry_calls = 0
        self.trampoline_calls = 0
        self.lv_calls = 0
        self.report_calls = 0
        self.emit_result_calls = 0
        self.stock_log_calls = 0
        self.stock_logs = []
        self.boot_run_calls = 0
        self.boot_return_pc = None
        self.boot_run_result = None
        self.init_return_pc = None
        self.stop_return_pc = None
        self.init_fault_injected = False
        self.stop_fault_injected = False
        self.app_calls = {"init": 0, "step": 0, "stop": 0}
        self.reports = []
        self.state_before_init = None
        self.allocations = []
        self.frees = []
        self.raw_next = HEAP_BASE + 4
        self.original_s = [0x51000010 + i * 0x111 for i in range(12)]
        self.original_sp = STACK_BASE + STACK_SIZE - 0x10
        self.original_gp = 0x62FC34C0
        self.initial_s4 = self.original_s[4]
        self.return_root = STACK_BASE + 0x1000 if root else 0
        self.minimum_sp = self.original_sp
        self.base = symbols["base"]
        self.app = symbols["app"]
        self.report_address = find_symbol(self.base, ("ap01_report",))
        self.emit_result_address = find_symbol(self.base, ("ap01_emit_result",))
        self.boot_run = find_symbol(self.base, ("ap01_boot_run", "boot_run"))
        self.stock_log = find_symbol(self.base, ("stock_log",))
        self.stub_addresses = {LV_OBJ_CREATE, STOCK_MALLOC, STOCK_FREE, self.stock_log}
        self.app_addresses = {
            find_symbol(self.app, ("ap01_diag_init", "diagnostic_init")): "init",
            find_symbol(self.app, ("ap01_diag_step", "diagnostic_step")): "step",
            find_symbol(self.app, ("ap01_diag_stop", "diagnostic_stop")): "stop",
        }
        self.base_entry = find_symbol(self.base, ("ap01_entry", "base_entry"))
        self.patch_pc = find_symbol(self.base, ("ap01_patch", "ap01_hook", "patch_entry"), False) or PATCH_PC
        self.trampoline = find_symbol(self.base, ("ap01_trampoline", "ap01_tramp"), False) or TRAMPOLINE

    def setup(self):
        rom_size = align_page(len(self.image))
        self.uc.mem_map(ROM_BASE, rom_size)
        self.uc.mem_write(ROM_BASE, bytes(self.image))
        self.uc.mem_map(STACK_BASE, STACK_SIZE)
        self.uc.mem_map(HEAP_BASE, HEAP_SIZE)
        self.uc.mem_write(HEAP_BASE, b"\xA5" * HEAP_SIZE)
        regs = self.u
        for index, reg in enumerate(regs["s"]):
            self.uc.reg_write(reg, self.original_s[index])
        self.uc.reg_write(regs["GP"], self.original_gp)
        self.uc.reg_write(regs["SP"], self.original_sp)
        self.uc.reg_write(regs["RA"], 0x6200FFF0)
        self.uc.reg_write(regs["A0"], 0)

        self.uc.hook_add(regs["hook_code"], self.on_code)

    def stop_with_error(self, message):
        self.error = message
        self.uc.emu_stop()

    def return_from_stub(self):
        ra = self.uc.reg_read(self.u["RA"])
        if ra == 0:
            self.stop_with_error("stub call had a null return address")
            return
        self.uc.reg_write(self.u["PC"], ra)

    def read_rom_string(self, address):
        rom_end = ROM_BASE + len(self.image)
        if not ROM_BASE <= address < rom_end:
            self.stop_with_error(f"stock_log string pointer is outside ROM: 0x{address:08x}")
            return ""
        raw = bytes(self.uc.mem_read(address, min(128, rom_end - address)))
        end = raw.find(b"\0")
        if end < 0:
            self.stop_with_error("stock_log string is not NUL-terminated within 128 bytes")
            return ""
        try:
            return raw[:end].decode("ascii")
        except UnicodeDecodeError:
            self.stop_with_error("stock_log string is not ASCII")
            return ""

    def on_code(self, uc, address, size, _user):
        self.instructions += 1
        self.minimum_sp = min(self.minimum_sp, uc.reg_read(self.u["SP"]))
        if self.instructions > MAX_INSTRUCTIONS:
            self.stop_with_error("instruction limit exceeded")
            return
        if (self.instructions & 0xFFF) == 0 and time.monotonic() - self.start_time > MAX_SECONDS:
            self.stop_with_error("wall-clock execution limit exceeded")
            return
        if address == CONTINUATION:
            self.reached_continuation = True
            uc.emu_stop()
            return
        if address == self.base_entry:
            self.base_entry_calls += 1
        if address == self.trampoline:
            self.trampoline_calls += 1
        if address == self.boot_run:
            self.boot_run_calls += 1
            self.boot_return_pc = uc.reg_read(self.u["RA"])
        if self.boot_return_pc is not None and address == self.boot_return_pc:
            self.boot_run_result = uc.reg_read(self.u["A0"])
        if address in self.app_addresses:
            app_name = self.app_addresses[address]
            self.app_calls[app_name] += 1
            if app_name == "init":
                self.state_before_init = bytes(uc.mem_read(uc.reg_read(self.u["A1"]), 16))
                self.init_return_pc = uc.reg_read(self.u["RA"])
            elif app_name == "stop":
                self.stop_return_pc = uc.reg_read(self.u["RA"])
        if self.init_return_pc is not None and address == self.init_return_pc:
            if self.fail_init and not self.init_fault_injected:
                uc.reg_write(self.u["A0"], 1)
                self.init_fault_injected = True
        if self.stop_return_pc is not None and address == self.stop_return_pc:
            if self.fail_stop and not self.stop_fault_injected:
                uc.reg_write(self.u["A0"], 1)
                self.stop_fault_injected = True
        if address == self.emit_result_address:
            self.emit_result_calls += 1
        if address == self.report_address:
            self.report_calls += 1
            self.reports.append({
                "code": uc.reg_read(self.u["A1"]),
                "value": uc.reg_read(self.u["A2"]),
            })
            return
        if address == self.stock_log:
            entry = {
                "level": uc.reg_read(self.u["A0"]),
                "file": self.read_rom_string(uc.reg_read(self.u["A1"])),
                "line": uc.reg_read(self.u["A2"]),
                "function": self.read_rom_string(uc.reg_read(self.u["A3"])),
                "format": self.read_rom_string(uc.reg_read(self.u["A4"])),
                "result": uc.reg_read(self.u["A5"]),
            }
            if self.error:
                return
            self.stock_log_calls += 1
            self.stock_logs.append(entry)
            if entry["level"] != 3 or entry["file"] != "ap01/app-xip.c" or entry["line"] != 1 or \
                    entry["function"] != "ap01_xip_abi1" or \
                    entry["format"] != "APX1 diag0031 result=%u (0=init-step-stop-ok)":
                self.stop_with_error("stock_log arguments do not match the bounded AP01 result contract")
                return
            self.return_from_stub()
            return
        if address not in self.stub_addresses:
            return
        if address == LV_OBJ_CREATE:
            self.lv_calls += 1
            if uc.reg_read(self.u["A0"]) != self.initial_s4:
                self.stop_with_error("stock lv_obj_create did not receive original s4")
                return
            uc.reg_write(self.u["A0"], self.return_root)
            self.return_from_stub()
        elif address == STOCK_MALLOC:
            ordinal = len(self.allocations) + 1
            requested = uc.reg_read(self.u["A0"])
            if self.fail_alloc_at == ordinal:
                uc.reg_write(self.u["A0"], 0)
            else:
                if requested == 0 or self.raw_next + requested > HEAP_BASE + HEAP_SIZE:
                    self.stop_with_error("malloc request escaped the emulated heap")
                    return
                raw = self.raw_next
                self.raw_next += requested + 4
                self.allocations.append({"raw": raw, "size": requested})
                uc.reg_write(self.u["A0"], raw)
            self.return_from_stub()
        elif address == STOCK_FREE:
            pointer = uc.reg_read(self.u["A0"])
            matches = [item for item in self.allocations if item["raw"] == pointer]
            if not matches or pointer in self.frees:
                self.stop_with_error("invalid or double free in emulated heap")
                return
            self.frees.append(pointer)
            self.return_from_stub()
        else:
            code = uc.reg_read(self.u["A1"])
            value = uc.reg_read(self.u["A2"])
            self.reports.append({"code": code, "value": value})
            self.return_from_stub()

    def run(self):
        self.setup()
        try:
            self.uc.emu_start(self.patch_pc, 0, count=MAX_INSTRUCTIONS)
        except Exception as exc:  # Unicorn errors include unmapped/invalid execution.
            self.error = f"Unicorn stopped with {type(exc).__name__}: {exc}"
        if self.error:
            raise TestFailure(self.error)
        if not self.reached_continuation:
            pc = self.uc.reg_read(self.u["PC"])
            raise TestFailure(f"patched hook did not reach continuation (pc=0x{pc:08x})")
        if self.base_entry_calls != 1:
            raise TestFailure(f"base entry executed {self.base_entry_calls} times")
        return self

    def checks(self):
        regs = self.u
        actual_s = [self.uc.reg_read(reg) for reg in regs["s"]]
        state_markers = []
        for allocation in self.allocations:
            state = (allocation["raw"] + 15) & ~15
            values = struct.unpack("<4I", bytes(self.uc.mem_read(state, 16)))
            state_markers.append({
                "base_version": values[0],
                "seed": values[1],
                "ready": values[2],
                "calls": values[3],
                "aligned": state % 16 == 0,
            })
        return {
            "continuation": self.reached_continuation,
            "base_entry_calls": self.base_entry_calls,
            "trampoline_calls": self.trampoline_calls,
            "lv_obj_create_calls": self.lv_calls,
            "report_calls": self.report_calls,
            "emit_result_calls": self.emit_result_calls,
            "stock_log_calls": self.stock_log_calls,
            "stock_logs": self.stock_logs,
            "boot_run_calls": self.boot_run_calls,
            "boot_run_result": self.boot_run_result,
            "init_fault_injected": self.init_fault_injected,
            "stop_fault_injected": self.stop_fault_injected,
            "app_calls": self.app_calls,
            "reports": self.reports,
            "allocations": len(self.allocations),
            "frees": len(self.frees),
            "instruction_count": self.instructions,
            "registers_preserved": actual_s == self.original_s,
            "sp_preserved": self.uc.reg_read(regs["SP"]) == self.original_sp,
            "sp_minimum": self.minimum_sp,
            "hook_callchain_stack_peak": self.original_sp - self.minimum_sp,
            "gp_preserved": self.uc.reg_read(regs["GP"]) == self.original_gp,
            "original_root": self.uc.reg_read(regs["A0"]) == self.return_root,
            "allocation_peak_bytes": max((x["size"] for x in self.allocations), default=0),
            "state_markers": state_markers,
            "state_before_init": (struct.unpack("<4I", self.state_before_init)
                                   if self.state_before_init is not None else None),
            "state_bss_zero_before_init": (
                self.state_before_init is not None and
                self.state_before_init[8:16] == b"\x00" * 8
            ),
            "state_bss_zero_after_stop": all(x["ready"] == 0 for x in state_markers),
        }


def require(condition, message):
    if not condition:
        raise TestFailure(message)


def require_result_sink(checks):
    expected = checks["boot_run_result"] if checks["boot_run_result"] is not None else 1
    require(checks["emit_result_calls"] == 1 and checks["stock_log_calls"] == 1,
            "native ap01_emit_result did not produce exactly one stock log")
    require(checks["stock_logs"][0]["result"] == expected,
            f"stock log result {checks['stock_logs'][0]['result']} != {expected}")


def run_success(unicorn, image, symbols):
    run = RomRun(unicorn, image, symbols)
    run.run()
    checks = run.checks()
    require(checks["lv_obj_create_calls"] == 1,
            "successful run did not preserve one original UI call")
    require(checks["trampoline_calls"] == 1, "patched site did not execute the expected trampoline")
    require(checks["boot_run_calls"] == 1 and checks["boot_run_result"] == 0,
            f"successful boot_run result was {checks['boot_run_result']!r}")
    require_result_sink(checks)
    require(checks["app_calls"] == {"init": 1, "step": 1, "stop": 1},
            f"unexpected diagnostic app calls: {checks['app_calls']}")
    require(checks["allocations"] == checks["frees"] == 1, "successful run leaked or double-freed state")
    require(checks["registers_preserved"] and checks["sp_preserved"] and checks["gp_preserved"],
            "callee-saved register, sp, or gp changed across the hook")
    require(checks["original_root"], "successful run did not return the original UI root")
    require(checks["report_calls"] == 3 and [x["code"] for x in checks["reports"]] == [1, 2, 3],
            f"unexpected diagnostic callbacks: {checks['reports']}")
    require([x["value"] for x in checks["reports"]] == [0x31, 8, 1],
            f"unexpected diagnostic callback values: {checks['reports']}")
    require(checks["state_bss_zero_before_init"],
            "App BSS was not zero after loading over poisoned heap memory")
    require(checks["state_markers"] and all(
        x["base_version"] == 0x31 and x["seed"] == 7 and x["aligned"]
        for x in checks["state_markers"]
    ), "App state data/BSS markers were not initialized by the native loader")
    require(u32(image, package_offset()) == 0x31585041, "candidate package magic is not at the XIP VA")
    return checks


def corruptions(image):
    offset = package_offset()
    length = package_length(image)
    cases = {}

    def mutate(name, fn, repair_header=False):
        candidate = bytearray(image)
        package = candidate[offset:offset + length]
        fn(package)
        if repair_header:
            repair_header_crc(package)
        candidate[offset:offset + len(package)] = package
        cases[name] = candidate

    mutate("header_crc_corrupt", lambda package: put_u32(package, 68, u32(package, 68) ^ 1))
    mutate("payload_crc_corrupt", lambda package: put_u32(package, 64, u32(package, 64) ^ 1), True)
    mutate("wrong_abi", lambda package: put_u32(package, 16, 2), True)
    mutate("wrong_base_sha", lambda package: package.__setitem__(72, package[72] ^ 1), True)
    mutate("entry_range", lambda package: put_u32(package, 36, u32(package, 32)), True)
    return cases


def run_root_null(unicorn, image, symbols):
    run = RomRun(unicorn, image, symbols, root=False)
    run.run()
    checks = run.checks()
    require(checks["app_calls"] == {"init": 0, "step": 0, "stop": 0},
            "null UI root still executed the App")
    require(checks["allocations"] == 0 and checks["frees"] == 0, "null UI root allocated App state")
    require(checks["original_root"], "null UI root was not preserved")
    require(checks["boot_run_calls"] == 0 and checks["boot_run_result"] is None,
            "null UI root unexpectedly entered boot_run")
    require_result_sink(checks)
    require(checks["trampoline_calls"] == 1 and checks["registers_preserved"] and
            checks["sp_preserved"] and checks["gp_preserved"],
            "null UI root changed the hook return context")
    return checks


def run_rejection_cases(unicorn, image, symbols):
    results = {}
    for name, candidate in corruptions(image).items():
        run = RomRun(unicorn, candidate, symbols)
        run.run()
        checks = run.checks()
        require(checks["app_calls"] == {"init": 0, "step": 0, "stop": 0},
                f"{name} executed the App")
        require(checks["boot_run_calls"] == 1 and checks["boot_run_result"] != 0,
                f"{name} did not return a nonzero boot_run error")
        require_result_sink(checks)
        require(checks["allocations"] == 0 and checks["frees"] == 0,
                f"{name} allocated despite package rejection")
        require(checks["lv_obj_create_calls"] == 1 and checks["original_root"],
                f"{name} did not continue the original UI path")
        require(checks["trampoline_calls"] == 1 and checks["registers_preserved"] and
                checks["sp_preserved"] and checks["gp_preserved"],
                f"{name} changed the hook return context")
        results[name] = checks
    return results


def run_seed_reject(unicorn, image, symbols):
    offset = package_offset()
    length = package_length(image)
    candidate = bytearray(image)
    package = candidate[offset:offset + length]
    data_offset = u32(package, 48)
    require(data_offset + 8 <= len(package), "diagnostic App package has no seed data")
    put_u32(package, data_offset + 4, 8)
    crc_package(package)
    candidate[offset:offset + length] = package
    run = RomRun(unicorn, candidate, symbols)
    run.run()
    checks = run.checks()
    require(checks["app_calls"] == {"init": 1, "step": 0, "stop": 1},
            f"odd-seed package did not reject during init: {checks['app_calls']}")
    require(checks["boot_run_calls"] == 1 and checks["boot_run_result"] != 0,
            "odd-seed package returned success from boot_run")
    require_result_sink(checks)
    require(checks["allocations"] == checks["frees"] == 1,
            "odd-seed rejection leaked or double-freed App state")
    require(checks["lv_obj_create_calls"] == 1 and checks["original_root"],
            "odd-seed rejection did not return through the original UI path")
    require(checks["trampoline_calls"] == 1 and checks["registers_preserved"] and
            checks["sp_preserved"] and checks["gp_preserved"],
            "odd-seed rejection changed the hook return context")
    return checks


def run_allocation_failures(unicorn, image, symbols):
    probe = RomRun(unicorn, image, symbols)
    probe.run()
    allocation_count = len(probe.allocations)
    require(allocation_count > 0, "successful run did not exercise an allocation")
    results = {}
    for ordinal in range(1, allocation_count + 1):
        run = RomRun(unicorn, image, symbols, fail_alloc_at=ordinal)
        run.run()
        checks = run.checks()
        require(checks["app_calls"] == {"init": 0, "step": 0, "stop": 0},
                f"allocation failure {ordinal} executed the App")
        require(checks["boot_run_calls"] == 1 and checks["boot_run_result"] != 0,
                f"allocation failure {ordinal} returned success from boot_run")
        require_result_sink(checks)
        require(checks["frees"] == checks["allocations"],
                f"allocation failure {ordinal} leaked or double-freed prior state")
        require(checks["lv_obj_create_calls"] == 1 and checks["original_root"],
                f"allocation failure {ordinal} did not fall through to the original UI")
        require(checks["trampoline_calls"] == 1 and checks["registers_preserved"] and
                checks["sp_preserved"] and checks["gp_preserved"],
                f"allocation failure {ordinal} changed the hook return context")
        results[f"malloc_fail_{ordinal}"] = checks
    return results


def run_callback_fault(unicorn, image, symbols, expected_result, *,
                       fail_init=False, fail_stop=False):
    run = RomRun(unicorn, image, symbols, fail_init=fail_init, fail_stop=fail_stop)
    run.run()
    checks = run.checks()
    require(checks["boot_run_calls"] == 1 and checks["boot_run_result"] == expected_result,
            f"callback fault boot_run result was {checks['boot_run_result']!r}, expected {expected_result}")
    expected_calls = {"init": 1, "step": 0 if fail_init else 1, "stop": 1}
    require(checks["app_calls"] == expected_calls,
            f"callback fault App calls were {checks['app_calls']}, expected {expected_calls}")
    require(checks["allocations"] == checks["frees"] == 1,
            "callback fault leaked or double-freed App state")
    require(checks["lv_obj_create_calls"] == 1 and checks["original_root"],
            "callback fault did not continue the original UI path")
    require(checks["trampoline_calls"] == 1 and checks["registers_preserved"] and
            checks["sp_preserved"] and checks["gp_preserved"],
            "callback fault changed the hook return context")
    require_result_sink(checks)
    if fail_init:
        require(checks["init_fault_injected"], "init fault was not injected")
    if fail_stop:
        require(checks["stop_fault_injected"], "stop fault was not injected")
    return checks


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path,
                        help="existing directory containing the parent build artifacts")
    parser.add_argument("--output", required=True, type=Path,
                        help="new JSON report path")
    parser.add_argument("--deps", type=Path,
                        help="existing isolated dependency directory containing Unicorn 2.1.4")
    args = parser.parse_args(argv)
    directory = args.directory.resolve()
    output = args.output.resolve()
    require(directory.is_dir(), f"--directory is not an existing directory: {directory}")
    require(not output.exists(), f"refusing to overwrite report: {output}")
    require(output.parent.is_dir(), f"report parent is not an existing directory: {output.parent}")
    files = artifact_files(directory)
    try:
        manifest = json.loads(files["manifest.json"].read_text(encoding="utf-8"))
        symbols = symbol_tables(json.loads(files["symbols.json"].read_text(encoding="utf-8")))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise TestFailure(f"invalid build metadata: {exc}") from exc
    image = bytearray(files["ap01-1.0.2_0031-app-xip.bin"].read_bytes())
    require(len(image) > package_offset() + PACKAGE_HEADER, "XIP image does not contain the App package")
    length = package_length(image)
    package = image[package_offset():package_offset() + length]
    require(u32(package, 0) == 0x31585041, "XIP image package magic is invalid")
    require(bytes(package[72:104]) == EXPECTED_BASE_SHA, "XIP App package has the wrong 0031 base SHA")
    candidate_manifest = manifest.get("candidate", {})
    require(candidate_manifest.get("size") == len(image),
            "candidate ROM size does not match manifest.json")
    rom_sha256 = hashlib.sha256(image).hexdigest()
    require(candidate_manifest.get("sha256") == rom_sha256,
            "candidate ROM SHA256 does not match manifest.json")
    require(files["app.apx"].read_bytes() == bytes(package),
            "app.apx is not byte-identical to the embedded XIP package")

    unicorn = load_unicorn(args.deps)
    cases = {"successful_start_step_stop": run_success(unicorn, image, symbols)}
    cases["lv_obj_create_null"] = run_root_null(unicorn, image, symbols)
    cases.update(run_rejection_cases(unicorn, image, symbols))
    cases["odd_seed_reject"] = run_seed_reject(unicorn, image, symbols)
    cases.update(run_allocation_failures(unicorn, image, symbols))
    cases["stop_failure_forced_release"] = run_callback_fault(
        unicorn, image, symbols, 25, fail_stop=True
    )
    cases["init_failure_forced_release"] = run_callback_fault(
        unicorn, image, symbols, 22, fail_init=True
    )
    cases["init_stop_failure_forced_release"] = run_callback_fault(
        unicorn, image, symbols, 23, fail_init=True, fail_stop=True
    )

    report = {
        "tool": "tools/ap01/test_app_rom.py",
        "tool_version": 1,
        "unicorn": "2.1.4",
        "directory": str(directory),
        "rom": {
            "path": files["ap01-1.0.2_0031-app-xip.bin"].name,
            "bytes": len(image),
            "sha256": hashlib.sha256(image).hexdigest(),
            "rom_xip_base": hex(ROM_BASE),
            "package_va": hex(PACKAGE_VA),
            "package_file_offset": hex(package_offset()),
            "package_bytes": length,
            "manifest_sha256_verified": True,
            "app_apx_embedded_verified": True,
        },
        "limits": {
            "max_instructions": MAX_INSTRUCTIONS,
            "max_seconds": MAX_SECONDS,
            "stack_base": hex(STACK_BASE),
            "stack_bytes": STACK_SIZE,
            "heap_base": hex(HEAP_BASE),
            "heap_bytes": HEAP_SIZE,
            "only_rv32_generated_code": True,
            "stock_calls_stubbed": [hex(LV_OBJ_CREATE), hex(STOCK_MALLOC), hex(STOCK_FREE)],
            "stock_log_stubbed": hex(find_symbol(symbols["base"], ("stock_log",))),
            "ap01_emit_result_native": True,
            "hardware": False,
        },
        "symbols": {
            "patch": hex(find_symbol(symbols["base"], ("ap01_patch", "ap01_hook", "patch_entry"), False) or PATCH_PC),
            "trampoline": hex(find_symbol(symbols["base"], ("ap01_trampoline", "ap01_tramp"), False) or TRAMPOLINE),
            "base_entry": hex(find_symbol(symbols["base"], ("ap01_entry", "base_entry"))),
            "app": {name: hex(address) for name, address in symbols["app"].items()
                    if name in {"ap01_diag_init", "ap01_diag_step", "ap01_diag_stop"}},
        },
        "tests": cases,
        "test_names": list(cases),
        "register_checks": {
            name: {
                "callee_saved": result["registers_preserved"],
                "sp": result["sp_preserved"],
                "gp": result["gp_preserved"],
                "original_root": result["original_root"],
            }
            for name, result in cases.items()
        },
        "memory_alloc_peaks": {
            name: result["allocation_peak_bytes"] for name, result in cases.items()
        },
        "stack_peaks": {
            name: {
                "sp_minimum": result["sp_minimum"],
                "hook_callchain_stack_peak": result["hook_callchain_stack_peak"],
            }
            for name, result in cases.items()
        },
        "counts": {
            "tests": len(cases),
            "passed": len(cases),
            "failed": 0,
        },
        "status": "pass",
        "manifest": manifest,
    }
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"status": "pass", "tests": len(cases), "output": str(output)}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except TestFailure as exc:
        print(f"AP01 ROM tests failed: {exc}", file=sys.stderr)
        raise SystemExit(2)
