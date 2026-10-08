#pragma once
#include <Arduino.h>
#include <ArduinoJson.h>
#include <utility>

// One authenticated, immutable read. No persistent bytes or empty-slot reserve.
class StatusPages {
public:
    static constexpr size_t MAX_BYTES = 16384;
    static constexpr size_t CHUNK_BYTES = 1800;
    void clear() { body_ = String(); token_ = String(); id_ = String(); until_ = 0; }
    void expire(uint64_t now) { if (until_ && now >= until_) clear(); }
    String begin(String &body, const String &token, const String &id, uint64_t now) {
        clear();
        if (body.length() > MAX_BYTES || id.length() != 32)
            return "{\"result\":\"rejected\",\"error\":\"status_limit\",\"stage\":\"status_read\"}";
        body_ = std::move(body); token_ = token; id_ = id; until_ = now + 15000;
        return read(token, id, 0, now);
    }
    String read(const String &token, const String &id, size_t offset, uint64_t now) {
        expire(now);
        if (!until_ || token != token_ || id != id_)
            return "{\"result\":\"rejected\",\"error\":\"status_read_expired\",\"stage\":\"status_read\"}";
        if (offset >= body_.length() || offset % CHUNK_BYTES)
            return "{\"result\":\"rejected\",\"error\":\"status_offset\",\"stage\":\"status_read\"}";
        size_t count = body_.length() - offset;
        if (count > CHUNK_BYTES) count = CHUNK_BYTES;
        String hex;
        hex.reserve(count * 2);
        static const char digits[] = "0123456789abcdef";
        for (size_t i = 0; i < count; ++i) {
            uint8_t byte = body_[offset+i]; hex += digits[byte >> 4]; hex += digits[byte & 15];
        }
        if (hex.length() != count * 2)
            return "{\"result\":\"rejected\",\"error\":\"allocation\",\"stage\":\"status_read\"}";
        JsonDocument doc;
        doc["result"] = "applied"; doc["status_read"] = id_;
        doc["offset"] = offset; doc["total_bytes"] = body_.length(); doc["chunk_hex"] = hex;
        doc["next_offset"] = offset + count < body_.length() ? offset + count : 0;
        String reply; serializeJson(doc, reply); return reply;
    }
private:
    String body_, token_, id_;
    uint64_t until_ = 0;
};
