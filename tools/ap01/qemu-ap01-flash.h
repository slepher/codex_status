/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit synthetic NOR backing and bounded BootROM API transport.
 * Not a recovered Boot2 image, real device dump or firmware-write service.
 * Parent owns bytes, initialized from its declared fixture; reset retains it.
 */
#ifndef QEMU_AP01_FLASH_H
#define QEMU_AP01_FLASH_H

#include "system/address-spaces.h"
#include "qemu/bswap.h"

typedef struct AP01Flash {
    uint8_t *bytes;
    size_t size;
    uint32_t args[6];
    unsigned args_written;
    uint32_t result;
    bool opt_active, state_saved, sbus2;
    bool continuous_read;
    bool burst_wrap, qspi_enabled, address_32bit, idbus_enabled;
    bool write_enabled, busy;
    bool ota_writable, fw1_active;
    uint32_t owner, idbus_io_mode, idbus_address, idbus_length;
    uint32_t xip_offset;
    uint64_t read_count, program_count, erase_count;
    uint64_t read_bytes, programmed_bytes, erased_bytes;
} AP01Flash;

static bool ap01_flash_ram(uint32_t address, size_t length)
{
    static const uint32_t bases[] = {
        0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000,
        0x88000000, 0xa8000000, 0xd8000000,
    };
    uint64_t end = (uint64_t)address + length;
    if (!length || end > UINT64_C(0x100000000)) { return false; }
    for (size_t i = 0; i < G_N_ELEMENTS(bases); i++) {
        if (address >= bases[i] && end <= (uint64_t)bases[i] +
            (i < 4 ? 0x78000 : 0x400000)) { return true; }
    }
    return false;
}

static bool ap01_flash_read_cfg(uint32_t address, void *buffer, size_t length)
{
    bool xip = address >= 0x9ffff000U &&
               (uint64_t)address + length <= UINT64_C(0xa0fff000);
    return (ap01_flash_ram(address, length) || xip) &&
        address_space_read(&address_space_memory, address,
                           MEMTXATTRS_UNSPECIFIED, buffer, length) == MEMTX_OK;
}

static bool ap01_flash_cfg(uint32_t address)
{
    uint8_t header[4];
    return ap01_flash_read_cfg(address, header, sizeof(header));
}

static void ap01_flash_reset(AP01Flash *s)
{
    /* Cold/warm functional transaction reset is not a factory erase. */
    memset(s->args, 0, sizeof(s->args));
    s->args_written = 0;
    s->result = 1; /* SDK BL_Err_Type ERROR; not generic success. */
    s->opt_active = s->state_saved = s->sbus2 = false;
    s->continuous_read = true;
    s->burst_wrap = s->qspi_enabled = s->address_32bit = false;
    s->write_enabled = s->busy = false;
    s->idbus_enabled = true;
    s->owner = 1; /* Fixed SDK OWNER_IAHB=1, OWNER_SAHB=0. */
    s->idbus_io_mode = s->idbus_address = s->idbus_length = 0;
    s->xip_offset = s->fw1_active ? 0x7b0000 : 0x10000;
    /* Diagnostic totals and programmed bytes survive transaction reset. */
}

static bool ap01_flash_bounds(AP01Flash *s, uint32_t address, uint32_t length)
{
    return s->bytes && length && (uint64_t)address + length <= s->size;
}

static bool ap01_flash_data_write_range(AP01Flash *s, uint32_t address, uint32_t length)
{
    if (s->ota_writable && length) {
        uint32_t inactive = s->fw1_active ? 0x10000 : 0x7b0000;
        uint32_t table = s->fw1_active ? 0xe000 : 0xf000;
        if ((address >= inactive && (uint64_t)address + length <= inactive + 0x700000) ||
            (address >= table && (uint64_t)address + length <= table + 0x1000)) {
            return true;
        }
    }
    return length && ((address >= 0x710000 &&
                       (uint64_t)address + length <= 0x720000) ||
                      (address >= 0xeb0000 &&
                       (uint64_t)address + length <= 0xff0000));
}

static MemTxResult ap01_flash_commit(AP01Flash *s, uint32_t slot)
{
    uint32_t *a = s->args;
    uint32_t result = 1;
    if (s->args_written != 63) { return MEMTX_ERROR; }
    switch (slot) {
    case 389: /* int SFlash_Write_Enable(cfg). */
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        s->write_enabled = true; result = 0;
        break;
    case 392: /* int SFlash_Busy(cfg). Synchronous leaf operations below
               * return only after bytes change, like SDK polling leaves. */
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        result = s->busy ? 1 : 0;
        break;
    case 368: case 369: case 371: case 384: {
        /* SDK bflb_sflash.c sector/32K/64K leaves internally write-enable,
         * execute, and poll before returning. Sector numbers are not byte
         * addresses. Erase(cfg,start,end) uses an inclusive ending address. */
        uint8_t cfg[16];
        uint64_t address, length;
        if (!ap01_flash_read_cfg(a[0], cfg, sizeof(cfg))) { return MEMTX_ERROR; }
        if (slot == 371) {
            address = a[1];
            length = a[2] >= a[1] ? (uint64_t)a[2] - a[1] + 1 : 0;
        } else {
            length = slot == 368 ? 32768 : slot == 369 ? 65536 : (uint32_t)cfg[12] * 1024;
            address = (uint64_t)a[1] * length;
        }
        if (cfg[12] != 4 || !length || length > 0x10000 ||
            address > UINT32_MAX || (address & 0xfff) || (length & 0xfff) ||
            !ap01_flash_bounds(s, address, length) ||
            !ap01_flash_data_write_range(s, address, length)) { break; }
        s->write_enabled = s->busy = true;
        memset(s->bytes + (uint32_t)address, 0xff, length);
        s->write_enabled = s->busy = false;
        s->erase_count++; s->erased_bytes += length; result = 0;
        break;
    }
    case 329: /* uint32 Get_Flash_Image_Offset(uint8 group,uint8 bank). */
        if (!a[0] && !a[1]) { result = s->xip_offset; }
        break;
    case 365: /* void Set_Flash_Image_Offset(uint32 offset,uint8 group,uint8 bank). */
        if (!a[1] && !a[2] && (a[0] == 0 || a[0] == (s->fw1_active ? 0x7b0000 : 0x10000))) {
            s->xip_offset = a[0];
            s->state_saved = a[0] == 0;
            if (!a[0]) { s->idbus_enabled = false; }
            result = 0;
        }
        break;
    case 367: /* void Set_Owner(uint8 owner): SAHB0 / IAHB1. */
        if (a[0] <= 1) { s->owner = a[0]; result = 0; }
        break;
    case 377: case 385: case 393: case 402: {
        uint8_t config[4];
        /* State-save leaf contracts: fixed SDK bflb_xip_sflash.c90..190,
         * bflb_sflash.c451/1206/1251/1293. No status-register success bits
         * are invented: this is explicit functional NOR mode state. */
        if (!ap01_flash_cfg(a[0]) ||
            address_space_read(&address_space_memory, a[0], MEMTXATTRS_UNSPECIFIED,
                               config, sizeof(config)) != MEMTX_OK) {
            return MEMTX_ERROR;
        }
        if (slot == 377) {
            if ((config[0] & 15) == 2 || (config[0] & 15) == 4) {
                s->qspi_enabled = true; result = 0;
            }
        } else if (slot == 385) {
            /* SDK rejects a flash config without the 32-bit-address flag;
             * the original save path ignores that return, so don't pretend
             * this 16MiB fixture needs/entered an unsupported mode. */
            if ((config[0] & 0x20) && a[1] <= 1) {
                s->address_32bit = a[1] != 0; result = 0;
            }
        } else if (slot == 393) {
            s->burst_wrap = false; result = 0;
        } else {
            if (!(config[0] & 0x10)) { s->burst_wrap = true; }
            result = 0;
        }
        break;
    }
    case 387: { /* int Set_IDbus_Cfg(cfg,io,cont,addr,len,bank). */
        uint8_t config[4];
        if (!ap01_flash_cfg(a[0]) ||
            address_space_read(&address_space_memory, a[0], MEMTXATTRS_UNSPECIFIED,
                               config, sizeof(config)) != MEMTX_OK) {
            return MEMTX_ERROR;
        }
        if (a[1] <= 4 && a[1] == (config[0] & 15) && a[2] <= 1 &&
            a[3] == 0 && a[4] == 32 && a[5] == 0 &&
            ((a[1] != 2 && a[1] != 4) || s->qspi_enabled)) {
            s->owner = 1;
            s->idbus_io_mode = a[1]; s->idbus_address = a[3];
            s->idbus_length = a[4]; s->continuous_read = a[2] != 0;
            s->idbus_enabled = true; result = 0;
        }
        break;
    }
    case 401: /* SFlash_Reset_Continue_Read(cfg): leave continuous mode. */
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        s->continuous_read = false;
        result = 0;
        break;
    case 320: { /* int SF_Cfg_Get_Flash_Cfg_Need_Lock(id, cfg*). */
        uint32_t crc = UINT32_MAX;
        const uint8_t *config;
        result = UINT32_MAX; /* This API uses -1, unlike BL_Err_Type APIs. */
        if (!ap01_flash_ram(a[1], 84)) { return MEMTX_ERROR; }
        if (!ap01_flash_bounds(s, 0x10008, 0x5c)) { break; }
        config = s->bytes + 0x1000c;
        if (a[0] != 0 && a[0] != 0x1840ef) { break; }
        if (memcmp(s->bytes + 0x10008, "FCFG", 4)) { break; }
        for (unsigned i = 0; i < 84; i++) {
            crc ^= config[i];
            for (unsigned bit = 0; bit < 8; bit++) {
                crc = (crc >> 1) ^ ((crc & 1) ? 0xedb88320U : 0);
            }
        }
        if ((crc ^ UINT32_MAX) != ldl_le_p(s->bytes + 0x10060)) { break; }
        if (address_space_write(&address_space_memory, a[1], MEMTXATTRS_UNSPECIFIED,
                                config, 84) != MEMTX_OK) { return MEMTX_ERROR; }
        result = 0;
        break;
    }
    case 358: /* SF_Ctrl_Sbus2_Replace */
        if (a[0] == 1) { s->sbus2 = true; result = 0; }
        break;
    case 359: /* SF_Ctrl_Sbus2_Revoke_replace: no input parameters. */
        s->sbus2 = false; result = 0;
        break;
    case 449: { /* XIP_SFlash_Opt_Enter(uint8_t *aesEnable). */
        uint8_t aes_disabled = 0;
        if (!ap01_flash_ram(a[0], 1)) { return MEMTX_ERROR; }
        if (!s->opt_active) {
            if (address_space_write(&address_space_memory, a[0],
                                    MEMTXATTRS_UNSPECIFIED, &aes_disabled, 1) != MEMTX_OK) {
                return MEMTX_ERROR;
            }
            s->opt_active = true; result = 0;
        }
        break;
    }
    case 450: /* XIP_SFlash_Opt_Exit(uint8_t aesEnable). */
        if (a[0] == 0 && s->opt_active) { s->opt_active = false; result = 0; }
        break;
    case 447: { /* State_Save(cfg, uint32_t *offset, group, bank). */
        uint8_t offset[4];
        if (!ap01_flash_cfg(a[0]) || !ap01_flash_ram(a[1], 4)) { return MEMTX_ERROR; }
        if (!a[2] && !a[3] && !s->state_saved && s->xip_offset == (s->fw1_active ? 0x7b0000 : 0x10000)) {
            stl_le_p(offset, s->xip_offset);
            if (address_space_write(&address_space_memory, a[1],
                                    MEMTXATTRS_UNSPECIFIED, offset, 4) != MEMTX_OK) {
                return MEMTX_ERROR;
            }
            s->state_saved = true; s->xip_offset = 0;
            s->owner = 0; s->continuous_read = s->burst_wrap = false;
            s->idbus_enabled = false; result = 0;
        }
        break;
    }
    case 446: /* State_Restore(cfg, offset, group, bank). */
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        if (a[1] == (s->fw1_active ? 0x7b0000 : 0x10000) && !a[2] && !a[3] && s->state_saved) {
            s->xip_offset = a[1]; s->state_saved = false;
            s->continuous_read = true; s->owner = 1;
            s->idbus_enabled = true; result = 0;
        }
        break;
    case 379: case 444: {
        /* SFlash_Read(cfg, io, cont, addr, out, len) or
         * XIP_SFlash_Read_Need_Lock(cfg, addr, out, len, group, bank). */
        uint32_t address = slot == 379 ? a[3] : a[1];
        uint32_t out = slot == 379 ? a[4] : a[2];
        uint32_t length = slot == 379 ? a[5] : a[3];
        uint8_t config[4];
        bool mode_ok;
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        if (address_space_read(&address_space_memory, a[0], MEMTXATTRS_UNSPECIFIED,
                               config, sizeof(config)) != MEMTX_OK) { return MEMTX_ERROR; }
        mode_ok = slot == 379 ?
            (a[1] <= 4 && a[1] == (config[0] & 15) && a[2] <= 1 &&
             ((a[1] != 2 && a[1] != 4) || s->qspi_enabled)) : (!a[4] && !a[5]);
        if (length && length <= 0x100000 && !ap01_flash_ram(out, length)) {
            return MEMTX_ERROR;
        }
        if (mode_ok && length <= 0x100000 && ap01_flash_bounds(s, address, length)) {
            if (address_space_write(&address_space_memory, out,
                                    MEMTXATTRS_UNSPECIFIED, s->bytes + address,
                                    length) != MEMTX_OK) { return MEMTX_ERROR; }
            s->read_count++; s->read_bytes += length; result = 0;
            if (slot == 379) { s->continuous_read = a[2] != 0; }
        }
        break;
    }
    case 374: case 448: {
        /* SFlash_Program(cfg,io,addr,data,len) or
         * XIP_SFlash_Write_Need_Lock(cfg,addr,data,len,group,bank). */
        uint8_t *input;
        uint8_t cfg[4];
        uint32_t address = slot == 374 ? a[2] : a[1];
        uint32_t data = slot == 374 ? a[3] : a[2];
        uint32_t length = slot == 374 ? a[4] : a[3];
        bool mode_ok;
        if (!ap01_flash_read_cfg(a[0], cfg, sizeof(cfg))) { return MEMTX_ERROR; }
        mode_ok = slot == 374 ?
            (a[1] <= 4 && a[1] == (cfg[0] & 15) &&
             ((a[1] != 2 && a[1] != 4) || s->qspi_enabled)) : (!a[4] && !a[5]);
        if (length && length <= 0x10000 && !ap01_flash_ram(data, length)) {
            return MEMTX_ERROR;
        }
        if (!mode_ok || length > 0x10000 ||
            !ap01_flash_bounds(s, address, length) ||
            !ap01_flash_data_write_range(s, address, length)) { break; }
        input = g_malloc(length);
        if (address_space_read(&address_space_memory, data, MEMTXATTRS_UNSPECIFIED,
                               input, length) != MEMTX_OK) {
            g_free(input); return MEMTX_ERROR;
        }
        for (uint32_t i = 0; i < length; i++) {
            if ((s->bytes[address + i] & input[i]) != input[i]) {
                g_free(input); input = NULL; break;
            }
        }
        if (input) {
            s->write_enabled = s->busy = true;
            memcpy(s->bytes + address, input, length);
            s->write_enabled = s->busy = false;
            g_free(input);
            s->program_count++; s->programmed_bytes += length; result = 0;
        }
        break;
    }
    case 440: /* XIP_SFlash_Erase_Need_Lock(cfg, addr, len, group, bank). */
        if (!ap01_flash_cfg(a[0])) { return MEMTX_ERROR; }
        if (!a[3] && !a[4] && a[2] <= 0x10000 && !(a[1] & 0xfff) &&
            !(a[2] & 0xfff) && ap01_flash_bounds(s, a[1], a[2]) &&
            ap01_flash_data_write_range(s, a[1], a[2])) {
            memset(s->bytes + a[1], 0xff, a[2]);
            s->erase_count++; s->erased_bytes += a[2]; result = 0;
        }
        break;
    default: return MEMTX_ERROR;
    }
    s->result = result;
    s->args_written = 0;
    return MEMTX_OK;
}

/* Parent maps this at F10000c0. Six fresh args, write18 slot commit, read1c
 * result. Bus/protocol errors are distinct from SDK domain ERROR=1. */
static MemTxResult ap01_flash_access(AP01Flash *s, hwaddr offset, uint64_t *value,
                                    unsigned size, bool write)
{
    if (size != 4 || (offset & 3)) { return MEMTX_ERROR; }
    if (offset < 0x18) {
        unsigned index = offset / 4;
        if (write) { s->args[index] = *value; s->args_written |= 1U << index; }
        else { *value = s->args[index]; }
        return MEMTX_OK;
    }
    if (offset == 0x18 && write) { return ap01_flash_commit(s, *value); }
    if (offset == 0x1c && !write) { *value = s->result; return MEMTX_OK; }
    return MEMTX_ERROR;
}
#endif
