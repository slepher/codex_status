#include "dev_log.h"

static const size_t DEV_LOG_CAP = 4096;
static char     logBuf[DEV_LOG_CAP];
static size_t   logWritten = 0;
static portMUX_TYPE logMux = portMUX_INITIALIZER_UNLOCKED;

DevLogger DevLog;

void DevLogger::append(const char *data, size_t len) {
    if (!len) return;
    portENTER_CRITICAL(&logMux);
    for (size_t i = 0; i < len; i++) {
        logBuf[logWritten % DEV_LOG_CAP] = data[i];
        logWritten++;
    }
    portEXIT_CRITICAL(&logMux);
    Serial.write(reinterpret_cast<const uint8_t *>(data), len);
}

void DevLogger::printf(const char *fmt, ...) {
    char buf[200];
    va_list args;
    va_start(args, fmt);
    int n = vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (n > 0) {
        append(buf, (size_t)((n < (int)sizeof(buf)) ? n : (int)sizeof(buf) - 1));
    }
}

void DevLogger::println(const char *s) {
    append(s, strlen(s));
    append("\n", 1);
}

void DevLogger::println() {
    append("\n", 1);
}

void DevLogger::print(const char *s) {
    append(s, strlen(s));
}

String DevLogger::dump() {
    portENTER_CRITICAL(&logMux);
    size_t len = logWritten < DEV_LOG_CAP ? logWritten : DEV_LOG_CAP;
    size_t start = logWritten < DEV_LOG_CAP ? 0 : logWritten % DEV_LOG_CAP;
    String out;
    out.reserve(len);
    for (size_t i = 0; i < len; i++) {
        out += logBuf[(start + i) % DEV_LOG_CAP];
    }
    portEXIT_CRITICAL(&logMux);
    return out;
}
