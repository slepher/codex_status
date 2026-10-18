/* Offline integration fixture: pixels are produced by the actual shared
 * CompiledTemplate renderer. No network/auth/flash outcome is simulated here. */
#include "ap01_template_lvgl.h"
static Ap01TemplateImage g_ap01_template_image;
static unsigned g_ap01_template_scene;
static unsigned g_ap01_template_active;
static unsigned ap01_template_sim_count(void) {
  unsigned count = 1;
  FILE *file = fopen("/ap01/template-count.txt", "r");
  if (file) { if (fscanf(file, "%u", &count) != 1) count = 0; fclose(file); }
  return count >= 1 && count <= 8 ? count : 0;
}
static bool ap01_template_sim_activate(void *unused, unsigned index) {
  (void)unused;
  FILE *fail = fopen("/ap01/template-activation-fail.txt", "r");
  if (fail) { fclose(fail); printf("SIM template activation rejected\n"); return false; }
  g_ap01_template_active = index;
  printf("SIM template_activate index=%u (fixture, no persistence)\n", index);
  return true;
}
static void ap01_template_sim_draw(void) {
  // NuttX's cache heap is allocated lazily. 4 KiB includes its allocator
  // metadata and the decoder's 1280-byte row; no full RGB image is cached.
  if (!lv_image_cache_is_enabled()) lv_image_cache_resize(4096, true);
  const char *name = g_ap01_template_scene ? "/ap01/both-windows.palette4" : "/ap01/weekly-only.palette4";
  FILE *file = fopen(name, "rb");
  uint8_t *pixels = ap01TemplateImageCandidate();
  bool ok = file && pixels && fread(pixels, 1, 38400, file) == 38400 && fgetc(file) == EOF;
  if (file) fclose(file);
  if (ok) ok = ap01TemplateImageCommit(&g_ap01_template_image, g_ap01_ui_root, pixels);
  if (!ok) {
    ap01TemplateImageDiscard(pixels);
    ap01_ui_label(g_ap01_ui_root, "Template unavailable", 40, 100, lv_color_white(), LV_FONT_DEFAULT);
    printf("ERROR stage=template_draw action=regenerate-shared-render-fixture\n");
  }
}
static void ap01_template_sim_tick(void) {
  unsigned scene = 0;
  FILE *file = fopen("/ap01/template-scene.txt", "r");
  if (file) { if (fscanf(file, "%u", &scene) != 1) scene = 0; fclose(file); }
  if (scene > 1 || scene == g_ap01_template_scene) return;
  g_ap01_template_scene = scene;
  printf("SIM template_data scene=%u focused=%u\n", scene, g_ap01_template_page.focused);
  if (g_ap01_template_page.focused) g_ap01_ui_redraw_pending = true;
}
