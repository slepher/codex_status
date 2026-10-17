OUTPUT_ARCH(riscv)
SECTIONS {
    . = 0x90001000;
    .api : { KEEP(*(.api)) }
    ASSERT(SIZEOF(.api) == 0x800, "API table must contain 512 words")
    . = 0x90001800;
    .pec_api : { KEEP(*(.pec_api)) }
    ASSERT(SIZEOF(.pec_api) == 0x210, "PEC API requires 16-byte header and 128 words")
    . = 0x90002000;
    .text : { *(.text*) *(.rodata*) }
    ASSERT(. <= 0x90011000, "shim exceeds reserved code range")
    .forbidden : { *(.data*) *(.sdata*) *(.bss*) *(.sbss*) *(COMMON) }
    ASSERT(SIZEOF(.forbidden) == 0, "shim cannot have writable data")
    /DISCARD/ : { *(.comment) *(.riscv.attributes) *(.eh_frame*) }
}
