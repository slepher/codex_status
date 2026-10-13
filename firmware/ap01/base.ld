OUTPUT_ARCH(riscv)
ENTRY(ap01_entry)
PROVIDE(stock_lv_obj_create = 0xA00C1EC6);
PROVIDE(stock_malloc = 0xA007E1C4);
PROVIDE(stock_free = 0xA007C256);
PROVIDE(stock_log = 0xA007CFA2);
SECTIONS
{
    .text 0xA02465D0 : {
        KEEP(*(.text.entry))
        *(.text .text.*)
        *(.rodata .rodata.*)
    }
    .app_package 0xA024C000 : {
        ap01_package_start = .;
        KEEP(*(.app_package))
        ap01_package_end = .;
    }
    .forbidden : { *(.data .data.* .sdata .sdata.* .bss .bss.* .sbss .sbss.* COMMON) }
    /DISCARD/ : { *(.comment .note* .riscv.attributes .eh_frame*) }
}
ASSERT(SIZEOF(.text) <= 0x5A30, "base overlaps App package")
ASSERT(SIZEOF(.forbidden) == 0, "implicit writable globals forbidden; use ABI state")
ASSERT(ap01_package_end <= 0xA02553D6, "payload exceeds verified GIF space")
