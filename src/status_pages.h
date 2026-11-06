#pragma once
#include <Arduino.h>
#include <ArduinoJson.h>
#include <utility>
#include "json_chunk.h"

// One authenticated, immutable read. No persistent bytes or empty-slot reserve.
class StatusPages {
public:
    static constexpr size_t MAX_BYTES = 16384;
    static constexpr size_t CHUNK_BYTES = 3600;
    void clear() { body_ = String(); token_ = String(); id_ = String(); until_ = 0; offset_ = next_ = 0; }
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
        if (offset >= body_.length() || (offset != 0 && offset != offset_ && offset != next_))
            return "{\"result\":\"rejected\",\"error\":\"status_offset\",\"stage\":\"status_read\"}";
        JsonDocument doc;
        doc["result"] = "applied"; doc["status_read"] = id_;
        doc["offset"] = offset; doc["total_bytes"] = body_.length();
        const size_t end = jsonChunk(doc, body_, offset, CHUNK_BYTES, 4096 - 48, true);
        if (end == offset)
            return "{\"result\":\"rejected\",\"error\":\"allocation\",\"stage\":\"status_read\"}";
        offset_ = offset; next_ = end;
        String reply; serializeJson(doc, reply); return reply;
    }
private:
    String body_, token_, id_;
    size_t offset_ = 0, next_ = 0;
    uint64_t until_ = 0;
};
