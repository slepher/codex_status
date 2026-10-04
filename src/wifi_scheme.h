#pragma once
#include "v2_state.h"
#include <memory>
#include <new>
#include <utility>

// Exact-length secret ownership. Empty fields allocate nothing. Allocation
// failures survive copies and are checked before a transaction can commit.
class WifiText {
    std::unique_ptr<char[]> text;
    bool okay = true;
public:
    WifiText() = default;
    WifiText(const WifiText &other) { set(other.c_str()); okay = okay && other.okay; }
    WifiText &operator=(const WifiText &other) { if (this != &other) { set(other.c_str()); okay = okay && other.okay; } return *this; }
    WifiText(WifiText &&other) noexcept : text(std::move(other.text)), okay(other.okay) {}
    WifiText &operator=(WifiText &&other) noexcept { clear(); text = std::move(other.text); okay = other.okay; return *this; }
    ~WifiText() { clear(); }
    void clear() { if (text) { volatile char *p = text.get(); for (size_t i = 0, n = strlen(text.get()); i < n; ++i) p[i] = 0; } text.reset(); okay = true; }
    bool set(const char *value) {
        size_t n = strlen(value);
        std::unique_ptr<char[]> replacement(n ? new(std::nothrow) char[n+1] : nullptr);
        if (n && !replacement) { okay = false; return false; }
        if (n) memcpy(replacement.get(), value, n+1);
        clear(); text = std::move(replacement); return true;
    }
    const char *c_str() const { return text ? text.get() : ""; }
    operator const char *() const { return c_str(); }
    bool valid() const { return okay; }
    size_t bytes() const { return text ? strlen(text.get())+1 : 0; }
};

template<class T> class WifiRows {
    std::unique_ptr<T[]> rows;
    size_t count = 0;
    bool okay = true;
public:
    WifiRows() = default;
    WifiRows(const WifiRows &other) { *this = other; }
    WifiRows &operator=(const WifiRows &other) {
        if (this == &other) return *this;
        std::unique_ptr<T[]> next(other.count ? new(std::nothrow) T[other.count] : nullptr);
        if (other.count && !next) { okay = false; return *this; }
        for (size_t i = 0; i < other.count; ++i) next[i] = other.rows[i];
        rows = std::move(next); count = other.count; okay = other.okay; return *this;
    }
    WifiRows(WifiRows &&other) noexcept : rows(std::move(other.rows)), count(other.count), okay(other.okay) { other.count = 0; }
    WifiRows &operator=(WifiRows &&other) noexcept { rows = std::move(other.rows); count = other.count; okay = other.okay; other.count = 0; return *this; }
    bool resize(size_t size) {
        std::unique_ptr<T[]> next(size ? new(std::nothrow) T[size] : nullptr);
        if (size && !next) { okay = false; return false; }
        for (size_t i = 0; i < count && i < size; ++i) next[i] = std::move(rows[i]);
        rows = std::move(next); count = size; return true;
    }
    bool append(const T &item) { if (!resize(count+1)) return false; rows[count-1] = item; return rows[count-1].valid(); }
    bool erase(size_t index) {
        // Allocate before moving so allocation failure cannot partially erase.
        std::unique_ptr<T[]> next(count > 1 ? new(std::nothrow) T[count-1] : nullptr);
        if (count > 1 && !next) { okay = false; return false; }
        for (size_t i = 0, j = 0; i < count; ++i) if (i != index) next[j++] = std::move(rows[i]);
        rows = std::move(next); --count; return true;
    }
    T &operator[](size_t i) { return rows[i]; }
    const T &operator[](size_t i) const { return rows[i]; }
    size_t size() const { return count; }
    T *begin() { return rows.get(); } T *end() { return count ? rows.get()+count : rows.get(); }
    const T *begin() const { return rows.get(); } const T *end() const { return count ? rows.get()+count : rows.get(); }
    bool valid() const { if (!okay) return false; for (const auto &r : *this) if (!r.valid()) return false; return true; }
};
struct WifiNetwork {
    WifiText bond, id, host, ssid, password, label;
    uint64_t version = 0;
    bool valid() const { return bond.valid() && id.valid() && host.valid() && ssid.valid() && password.valid() && label.valid(); }
    size_t heapBytes() const { return bond.bytes()+id.bytes()+host.bytes()+ssid.bytes()+password.bytes()+label.bytes(); }
};
struct WifiSchemes {
    WifiRows<WifiNetwork> networks;
    WifiText selectedBond, selectedId;
    int find(const char *bond, const char *id) const {
        for (size_t i = 0; i < networks.size(); ++i) if (!strcmp(networks[i].bond, bond) && !strcmp(networks[i].id, id)) return (int)i;
        return -1;
    }
    const char *put(const WifiNetwork &network, bool replace) {
        if (!valid() || !network.valid()) return "memory";
        int slot = find(network.bond, network.id), count = 0, bonds = 0;
        if (slot >= 0 && !replace) return "exists";
        if (slot < 0 && replace) return "not_found";
        for (size_t i = 0; i < networks.size(); ++i) {
            const auto &n = networks[i];
            if (!strcmp(n.bond, network.bond)) { ++count; if ((int)i != slot && !strcmp(n.host, network.host)) return "mapping_conflict"; }
            bool first = true;
            for (size_t j = 0; j < i; ++j) if (!strcmp(networks[j].bond, n.bond)) first = false;
            if (first) ++bonds;
        }
        if (slot < 0 && (networks.size() >= 12 || count >= 4 || (!count && bonds >= 3))) return "capacity";
        if (slot < 0) { if (!networks.append(network)) return "memory"; }
        else networks[slot] = network;
        selectedBond.set(network.bond); selectedId.set(network.id);
        return valid() ? nullptr : "memory";
    }
    bool erase(int slot) {
        bool selected = !strcmp(selectedBond, networks[slot].bond) && !strcmp(selectedId, networks[slot].id);
        if (!networks.erase(slot)) return false;
        if (selected) { selectedBond.clear(); selectedId.clear(); } return true;
    }
    bool valid() const { return networks.valid() && selectedBond.valid() && selectedId.valid(); }
};
struct WifiReceipt {
    WifiText bridge, request, bond, error;
    uint8_t hash[32] = {};
    uint64_t revision = 0;
    bool valid() const { return bridge.valid() && request.valid() && bond.valid() && error.valid(); }
    size_t heapBytes() const { return bridge.bytes()+request.bytes()+bond.bytes()+error.bytes(); }
};
struct WifiStore {
    WifiSchemes schemes;
    // Only bond/id are retained for an unedited active row. Before editing or
    // deleting that version, preserve just its old SSID/password for next wake.
    WifiNetwork active;
    uint64_t revision = 0, activeVersion = 0;
    WifiRows<WifiReceipt> receipts;
    const WifiNetwork *activeCredentials() const {
        int slot = schemes.find(active.bond, active.id);
        return slot >= 0 && schemes.networks[slot].version == activeVersion ? &schemes.networks[slot] : &active;
    }
    void preserveActive() {
        if (!*active.id) return;
        const auto *n = activeCredentials();
        if (n != &active) { active.ssid.set(n->ssid); active.password.set(n->password); }
    }
    void selectActive(int slot) {
        active = WifiNetwork{};
        if (slot >= 0) { active.bond.set(schemes.networks[slot].bond); active.id.set(schemes.networks[slot].id); }
        activeVersion = slot >= 0 ? schemes.networks[slot].version : 0;
    }
    bool valid() const { return schemes.valid() && active.valid() && receipts.valid(); }
    size_t heapBytes() const {
        size_t bytes = schemes.networks.size()*sizeof(WifiNetwork)+receipts.size()*sizeof(WifiReceipt)+active.heapBytes()+schemes.selectedBond.bytes()+schemes.selectedId.bytes();
        for (const auto &n : schemes.networks) bytes += n.heapBytes();
        for (const auto &r : receipts) bytes += r.heapBytes(); return bytes;
    }
};
