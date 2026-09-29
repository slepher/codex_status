#pragma once

#include <stdint.h>

// A failed partial attempt can be recovered by a successful full write.
// Report the final frame, while keeping the busy-failure counter diagnostic.
inline uint8_t displayStateAfterRender(uint32_t writesBefore, uint32_t writesAfter,
                                       uint32_t busyBefore, uint32_t busyAfter) {
    return writesAfter != writesBefore || busyAfter == busyBefore ? 1 : 3;
}
