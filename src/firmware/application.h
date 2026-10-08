#pragma once

namespace firmware::application {
void initialize(const char *firmwareVersion, const char *imageIdentity);
void tick();
} // namespace firmware::application
