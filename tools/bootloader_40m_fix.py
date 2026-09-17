# Pre-build fix for the board's flash frequency.
#
# pioarduino ships prebuilt bootloader ELFs only for 80m/120m flash speeds.
# This board needs 40 MHz (its GD25Q64 returns a wrong JEDEC ID and fails page
# programs at 80 MHz), so set board_build.f_flash = 40000000L and have this
# script provide the missing `bootloader_<mode>_40m.elf`.
#
# The ELF is only the bootloader code; the actual flash clock is written into
# the generated bootloader image header by esptool/elf2image according to the
# configured frequency, so reusing the 80m ELF is fine here.
#
# Note: a framework/libs reinstall wipes the copied file; this script runs on
# every build and restores it.
import os
import shutil

Import("env")  # noqa: F821

libs_dir = env.PioPlatform().get_package_dir("framework-arduinoespressif32-libs")  # noqa: F821
if libs_dir:
    bin_dir = os.path.join(libs_dir, "esp32s3", "bin")
    for mode in ("qio", "dio", "opi"):
        src = os.path.join(bin_dir, "bootloader_%s_80m.elf" % mode)
        dst = os.path.join(bin_dir, "bootloader_%s_40m.elf" % mode)
        if os.path.isfile(src) and not os.path.isfile(dst):
            shutil.copy2(src, dst)
            print("bootloader_40m_fix: created %s" % os.path.basename(dst))
