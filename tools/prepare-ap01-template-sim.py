"""Patch an isolated, existing AP01 rehost build; never edits the archive.

Usage: python prepare-ap01-template-sim.py NATIVE_SCRIPTS ISOLATED_SIM_ROOT
The caller must precreate/copy the isolated SIM using an elevated command.
"""
from pathlib import Path
import shutil
import subprocess
import sys

repo = Path(__file__).resolve().parents[1]
live = '--live' in sys.argv
native, sim = (Path(arg).resolve() for arg in sys.argv[1:] if arg != '--live')
destination = sim / 'apps/examples/lvgldemo'
assert destination.is_dir() and (sim / 'nuttx/.config').is_file()
assert sim.name.startswith('nuttx-ap01-template-'), 'Only isolated integration builds may be patched'
# Rebase copied configuration links inside this isolated copy. NuttX app
# object names encode absolute paths; a copied archive can otherwise retain
# both old and new main objects and link the old UI successfully.
old_root = '/home/slepher/nuttx-ap01-sim'
for link in sim.rglob('*'):
    if link.is_symlink():
        target = str(link.readlink())
        if target.startswith(old_root + '/'):
            link.unlink()
            link.symlink_to(str(sim) + target[len(old_root):])
archive = sim / 'apps/libapps.a'
if archive.is_file():
    members = subprocess.check_output(['ar', 't', str(archive)], text=True).splitlines()
    old_tag, new_tag = old_root.strip('/').replace('/', '.'), str(sim).strip('/').replace('/', '.')
    stale = [name for name in members if old_tag in name and name.replace(old_tag, new_tag) in members]
    if stale:
        subprocess.run(['ar', 'd', str(archive), *stale], check=True)
        print(f'Retired {len(stale)} duplicate members in isolated copied archive')
for pattern in ('ap01-ui*.h', 'ap01-sim*.h'):
    for source in native.glob(pattern):
        shutil.copyfile(source, destination / source.name)
shutil.copyfile(native / 'nuttx-sim.c', destination / 'lvgldemo.c')
for name in ('ap01_template_page.h', 'ap01_template_display.h', 'ap01_template_lvgl.h'):
    shutil.copyfile(repo / 'src' / name, destination / name)
shutil.copyfile(repo / ('tools/ap01-platform-sim.h' if live else 'tools/ap01-template-sim.h'), destination / 'ap01-template-sim.h')
for scene in ('weekly-only', 'both-windows'):
    shutil.copyfile(repo / f'artifacts/ap01-template-integration/{scene}.palette4', sim / f'{scene}.palette4')
path = destination / 'ap01-ui.h'
text = path.read_text()
def replace(old, new):
    global text
    assert text.count(old) == 1, f'Unexpected native source at: {old[:65]!r}'
    text = text.replace(old, new)

replace('#include "ap01-assets.h"', '#include "ap01-assets.h"\n#include "ap01_template_page.h"\nstatic Ap01TemplatePage g_ap01_template_page;')
replace('#include "ap01-sim-control.h"', '#include "ap01-template-sim.h"\n#include "ap01-sim-control.h"')
text = text.replace('ap01_ui_page_name(g_ap01_ui_page)', '(g_ap01_template_page.focused ? "Codex" : ap01_ui_page_name(g_ap01_ui_page))')
replace('  if (g_ap01_ui_root == NULL) return;\n  g_ap01_ui_redraw_pending', '  if (g_ap01_ui_root == NULL) return;\n  ap01TemplateImageClose(&g_ap01_template_image);\n  g_ap01_ui_redraw_pending')
replace('  else ap01_ui_draw_overview();', '  else if (g_ap01_template_page.focused) ap01_template_sim_draw();\n  else ap01_ui_draw_overview();')
replace('  if (redraw) g_ap01_ui_redraw_pending = true;', '  ap01_template_sim_tick();\n  if (redraw) g_ap01_ui_redraw_pending = true;')
replace('      bool c1 = g_ap01_sim.banks[0].connected;', '''      bool exiting_previous = false;
      if (g_ap01_template_page.focused) {
        Ap01TemplateMove move = ap01TemplateRotate(&g_ap01_template_page,
          ap01_template_sim_count(), direction, ap01_template_sim_activate, NULL);
        if (move == AP01_TEMPLATE_REJECTED) return;
        if (move == AP01_TEMPLATE_CHANGED) { ap01_ui_redraw(); return; }
        g_ap01_ui_page = AP01_UI_PET;
        if (move == AP01_TEMPLATE_EXIT_NEXT) { ap01_ui_redraw(); return; }
        exiting_previous = true;
      }
      enum ap01_ui_page_e previous = g_ap01_ui_page;
      bool c1 = g_ap01_sim.banks[0].connected;''')
replace('''      g_ap01_ui_selection = 0;
    }
  ap01_ui_redraw();
}

static void ap01_ui_press''', '''      if ((direction > 0 && g_ap01_ui_page == AP01_UI_PET && previous != AP01_UI_PET) ||
          (direction < 0 && previous == AP01_UI_PET && !exiting_previous)) {
        if (ap01_template_sim_count()) {
          ap01TemplateEnter(&g_ap01_template_page, ap01_template_sim_count(),
            g_ap01_template_active, direction, ap01_template_sim_activate, NULL);
          g_ap01_ui_page = previous;
        }
      }
      g_ap01_ui_selection = 0;
    }
  ap01_ui_redraw();
}

static void ap01_ui_press''')
replace('''  else if (g_ap01_ui_page == AP01_UI_POWER ||''', '''  else if (g_ap01_template_page.focused && g_ap01_ui_mode == AP01_UI_MODE_OVERVIEW) return;
  else if (g_ap01_ui_page == AP01_UI_POWER ||''')
replace('''  else if (g_ap01_ui_mode == AP01_UI_MODE_DETAIL && g_ap01_ui_page == AP01_UI_POWER &&''', '''  else if (g_ap01_template_page.focused && g_ap01_ui_mode == AP01_UI_MODE_OVERVIEW)
    g_ap01_template_page.focused = false;
  else if (g_ap01_ui_mode == AP01_UI_MODE_DETAIL && g_ap01_ui_page == AP01_UI_POWER &&''')
replace('''  g_ap01_ui_page = g_ap01_sim.charger_connected ? AP01_UI_POWER :''', '''  g_ap01_template_page.focused = false;
  g_ap01_ui_page = g_ap01_sim.charger_connected ? AP01_UI_POWER :''')
replace('''  /* Pure UI restart. The initiating service owns committing firmware/reset. */''', '''  /* Pure UI restart. The initiating service owns committing firmware/reset. */
  g_ap01_template_page.focused = false;''')
if live:
    replace('  lv_obj_invalidate(g_ap01_ui_root);\n}', '  lv_obj_invalidate(g_ap01_ui_root);\n  ap01_live_after_refresh();\n}')
    replace('  /* Explicit SIM backlight presentation:', '  ap01_live_pair_draw();\n  /* Explicit SIM backlight presentation:')
    replace('  lv_event_code_t code = lv_event_get_code(event);', '  if (ap01_live_input(event)) return;\n  lv_event_code_t code = lv_event_get_code(event);')
path.write_text(text)
path = destination / 'ap01-sim-control.h'
path.write_text(path.read_text().replace('ap01_ui_page_name(g_ap01_ui_page)', '(g_ap01_template_page.focused ? "Codex" : ap01_ui_page_name(g_ap01_ui_page))'))
path = destination / 'ap01-sim-service-impl.h'
text = path.read_text()
assert text.count('fs=/home/slepher/nuttx-ap01-sim') == 1
path.write_text(text.replace('fs=/home/slepher/nuttx-ap01-sim', 'fs=' + str(sim)))
print(f'Patched isolated UI: {destination}; native directed edges and overlay order retained')
