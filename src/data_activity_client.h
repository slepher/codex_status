#pragma once
#include "data_activity_led.h"
#include <NetworkClient.h>

// Count actual application bytes, including a request whose response times out.
class DataActivityClient : public NetworkClient {
public:
    using NetworkClient::read;
    using NetworkClient::write;
    int read(uint8_t *buffer, size_t size) override {
        const int count = NetworkClient::read(buffer, size);
        if (count > 0) dataLedActivity();
        return count;
    }
    size_t write(const uint8_t *buffer, size_t size) override {
        const size_t count = NetworkClient::write(buffer, size);
        if (count) dataLedActivity();
        return count;
    }
};
