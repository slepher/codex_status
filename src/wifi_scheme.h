#pragma once
#include "v2_state.h"

// One NVS blob; credentials never enter a public status projection.
struct WifiNetwork {
    char bond[22] = {}; // identity address/type
    char id[33] = {};
    char host[33] = {};
    char ssid[33] = {};
    char password[65] = {};
};
struct WifiSchemes {
    uint32_t version = 1;
    WifiNetwork networks[12] = {};
    char selectedBond[22] = {};
    char selectedId[33] = {};
    uint32_t crc = 0;

    int find(const char *bond, const char *id) const {
        for (int i = 0; i < 12; ++i)
            if (*networks[i].id && !strcmp(networks[i].bond, bond) &&
                !strcmp(networks[i].id, id)) return i;
        return -1;
    }
    const char *put(const WifiNetwork &network, bool replace) {
        int slot = find(network.bond, network.id), free = -1, count = 0, bonds = 0;
        if (slot >= 0 && !replace) return "exists";
        if (slot < 0 && replace) return "not_found";
        for (int i = 0; i < 12; ++i) {
            const auto &n = networks[i];
            if (!*n.id) { if (free < 0) free = i; continue; }
            bool same = !strcmp(n.bond, network.bond);
            if (same) {
                ++count;
                if (i != slot && !strcmp(n.host, network.host)) return "mapping_conflict";
            }
            bool first = true;
            for (int j = 0; j < i; ++j)
                if (*networks[j].id && !strcmp(networks[j].bond, n.bond)) first = false;
            if (first) ++bonds;
        }
        if (slot < 0 && (free < 0 || count >= 4 || (!count && bonds >= 3))) return "capacity";
        networks[slot < 0 ? free : slot] = network;
        strcpy(selectedBond, network.bond);
        strcpy(selectedId, network.id);
        return nullptr;
    }
    void erase(int slot) {
        if (!strcmp(selectedBond, networks[slot].bond) &&
            !strcmp(selectedId, networks[slot].id)) selectedBond[0] = selectedId[0] = 0;
        networks[slot] = WifiNetwork{};
    }
    void seal() { crc = v2Crc32((const uint8_t *)this, offsetof(WifiSchemes, crc)); }
    bool valid() const {
        if (version != 1 || crc != v2Crc32((const uint8_t *)this, offsetof(WifiSchemes, crc)) ||
            selectedBond[21] || selectedId[32]) return false;
        for (const auto &n : networks)
            if (n.bond[21] || n.id[32] || n.host[32] || n.ssid[32] || n.password[64]) return false;
        return !*selectedId || find(selectedBond, selectedId) >= 0;
    }
};

// v1 remains byte-for-byte intact for migration; v2 stores active credentials
// separately so saving an edited network cannot switch it on the next wake.
struct WifiReceipt {
    char bridge[65] = {};
    char request[65] = {};
    char bond[22] = {};
    uint8_t hash[32] = {};
    char error[33] = {};
    uint64_t revision = 0;
};
struct WifiStoreV2 {
    uint32_t schema = 2;
    WifiSchemes schemes;
    WifiNetwork active;
    uint64_t revision = 0;
    uint64_t activeVersion = 0;
    uint64_t versions[12] = {};
    char labels[12][65] = {};
    WifiReceipt receipts[8];
    uint32_t crc = 0;
    void seal() {
        schemes.seal();
        crc = v2Crc32((const uint8_t *)this, offsetof(WifiStoreV2, crc));
    }
    bool valid() const {
        if (schema != 2 || !schemes.valid() || crc != v2Crc32((const uint8_t *)this, offsetof(WifiStoreV2, crc))) return false;
        if (active.bond[21] || active.id[32] || active.host[32] || active.ssid[32] || active.password[64]) return false;
        for (int i = 0; i < 12; ++i) if (labels[i][64]) return false;
        for (const auto &r : receipts) if (r.bridge[64] || r.request[64] || r.bond[21] || r.error[32]) return false;
        return true;
    }
};
