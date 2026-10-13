OUTPUT_ARCH(riscv)
ENTRY(ap01_diag_init)
SECTIONS
{
    .text 0xA024C070 : { *(.text .text.*) *(.rodata .rodata.*) }
    .forbidden : { *(.data .data.* .sdata .sdata.* .bss .bss.* .sbss .sbss.* COMMON) }
    /DISCARD/ : { *(.comment .note* .riscv.attributes .eh_frame*) }
}
ASSERT(SIZEOF(.forbidden) == 0, "App globals forbidden; use initialized ABI workspace")
