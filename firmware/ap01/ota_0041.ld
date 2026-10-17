OUTPUT_ARCH(riscv)
ENTRY(ota_0041_main)
SECTIONS
{
  . = 0xa01c6000;
  .ota_manifest : { KEEP(*(.ota_manifest)) }
  ASSERT(SIZEOF(.ota_manifest) == 256, "OTA manifest must be256 bytes")
  . = 0xa01c6100;
  .text : { KEEP(*(.text.entry)) *(.text .text.*) }
  .rodata : { *(.rodata .rodata.* .srodata .srodata.*) }
  .forbidden : { *(.data .data.* .bss .bss.* .sdata .sdata.* .sbss .sbss.* COMMON) }
  ASSERT(SIZEOF(.forbidden) == 0, "OTA port must have no writable static state")
  /DISCARD/ : { *(.comment .note* .eh_frame*) }
}
PROVIDE(snprintf = 0xa002d7fc);
