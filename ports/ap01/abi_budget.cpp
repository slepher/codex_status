// Compile-time RV32 measurements, never a firmware entry or a loadable ROM.
#include "ap01_binding.h"
#include "template_renderer.h"
extern "C" {
__attribute__((section(".ap01_budget"),used))
extern const uint32_t ap01_binding_budget[] = {
    sizeof(void *),sizeof(Ap01Bindings),sizeof(Ap01Binding),8,
    16, // APB1 header, including an empty repository.
    5+64+96+4+32+64, // Maximum explicit bytes per binding.
    38400, // One packed 320x240 palette4 frame.
};
__attribute__((section(".ap01_render_budget"),used))
extern const uint32_t ap01_render_budget[] = {
    sizeof(PAINT), sizeof(TemplateRenderer), sizeof(CtTemplate),
};
}
