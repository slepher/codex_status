// Native-port host validation only: exercises the exact native String, owned
// renderer and immutable fonts. This is not an AP01 entry point or ROM.
#include "template_renderer.h"
#include "font_asset.h"
#include <memory>

static std::unique_ptr<uint8_t[]> readFile(const char *path, size_t &length) {
    FILE *f = fopen(path, "rb");
    if (!f || fseek(f, 0, SEEK_END)) { if (f) fclose(f); return {}; }
    long n = ftell(f);
    if (n <= 0 || n > 1024 * 1024 || fseek(f, 0, SEEK_SET)) { fclose(f); return {}; }
    std::unique_ptr<uint8_t[]> bytes(new (std::nothrow) uint8_t[n + 1]);
    bool ok = bytes && fread(bytes.get(), 1, n, f) == (size_t)n;
    fclose(f);
    if (!ok) return {};
    bytes[n] = 0; length = n; return bytes;
}
int main(int argc, char **argv) {
    if (argc != 7 && argc != 8) return 2;
    String self("AP01"); self += self;
    self += self.c_str() + 4;
    String copy(self); copy = copy.substring(4);
    String moved(std::move(copy));
    if (self != "AP01AP01AP01" || moved != "AP01AP01" || copy.length() ||
        String(-9223372036854775807LL) != "-9223372036854775807" ||
        String(18446744073709551615ULL) != "18446744073709551615") return 11;
    FontAssetInfo gray = {};
    gray.pixelFormat = FONT_PIXEL_GRAY4;
    String mismatch;
    if (fontAssetMatchesTarget(gray, mismatch) || mismatch != "font_pixel_format_mismatch") return 12;
    size_t n = 0;
    auto templateBytes = readFile(argv[1], n);
    if (!templateBytes) return 3;
    String templateJson((char *)templateBytes.get(), n);
    auto usageBytes = readFile(argv[2], n);
    if (!usageBytes) return 4;
    String usage((char *)usageBytes.get(), n);
    auto frame = std::unique_ptr<uint8_t[]>(new (std::nothrow) uint8_t[38400]);
    auto ct = std::unique_ptr<CtTemplate>(new (std::nothrow) CtTemplate);
    if (!frame || !ct) return 5;
    PAINT paint = {};
    TemplateRenderer renderer(paint, 320, 240);
    renderer.Paint_NewImage(frame.get(), 320, 240, 0, 0);
    renderer.Paint_SetScale(16);
    renderer.Paint_Clear(0);
    std::unique_ptr<uint8_t[]> fonts[3];
    for (int i = 0; i < 3; ++i) {
        fonts[i] = readFile(argv[4 + i], n);
        if (!fonts[i] || !renderer.tplFontBindAsset(fonts[i].get(), n)) return 6;
    }
    String error;
    if (!renderer.tplCompile(templateJson, *ct, error)) { fprintf(stderr, "%s\n", error.c_str()); return 7; }
    TplEnv env;
    env.channel = "WIFI"; env.ip = "192.168.1.50"; env.syncHHMM = "12:40";
    env.battery = 78; env.wifiState = "connected"; env.mode = "light";
    if (argc == 8) {
        if (strcmp(argv[7], "--native-rom")) return 2;
        env = TplEnv{}; env.channel = "WIFI"; env.battery = -1;
        env.syncHHMM = "--:--"; env.mode = "always_on";
    }
    if (!renderer.tplDrawCt(*ct, usage, env)) return 8;
    FILE *out = fopen(argv[3], "wb");
    if (!out) return 9;
    bool ok = fwrite(frame.get(), 1, 38400, out) == 38400;
    if (fclose(out)) ok = false;
    renderer.tplFontClearAssets();
    return ok ? 0 : 10;
}
