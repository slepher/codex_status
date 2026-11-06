#pragma once
#include <Arduino.h>
#include <ArduinoJson.h>
#include <algorithm>

// Bound the serialized JSON, including escaping, without splitting UTF-8.
inline size_t jsonChunk(JsonDocument &doc, const String &body, size_t offset,
                        size_t rawLimit, size_t wireLimit, bool zeroAtEnd = false) {
    if (offset >= body.length() || (((uint8_t)body[offset] & 0xc0) == 0x80)) return offset;
    size_t end = std::min(body.length(), offset + rawLimit);
    while (end > offset) {
        while (end < body.length() && end > offset && (((uint8_t)body[end] & 0xc0) == 0x80)) --end;
        if (end == offset) break;
        doc["chunk"] = body.substring(offset, end);
        JsonString chunk = doc["chunk"].as<JsonString>();
        if (!chunk || chunk.size() != end - offset) break;
        doc["next_offset"] = zeroAtEnd && end == body.length() ? 0 : end;
        const size_t size = measureJson(doc);
        if (!doc.overflowed() && size <= wireLimit) return end;
        if (doc.overflowed()) break;
        end -= std::min(end - offset, std::max(size_t(1), size - wireLimit));
    }
    doc.remove("chunk");
    return offset;
}
