OUTPUT_ARCH(riscv)
ENTRY(ap01Register0041)
SECTIONS {
  /* Start beyond the entire immutable original, including its PEC extent.
     The builder moves only the final recovery record to the new image end. */
  . = 0xa0674000;
  .manifest : { KEEP(*(.ap01_manifest)) }
  ASSERT(SIZEOF(.manifest)==256,"Native manifest must be 256 bytes")
  .text : { KEEP(*(.text.ap01Register0041)) *(.text .text.*) }
  .rodata : { *(.rodata .rodata.* .srodata .srodata.*) }
  .abi : { KEEP(*(.ap01_abi)) }
  .forbidden : {
    *(.data .data.* .bss .bss.* .sdata .sdata.* .sbss .sbss.* COMMON)
    *(.got .got.* .tdata .tdata.* .tbss .tbss.*)
    *(.init_array .init_array.* .fini_array .fini_array.* .ctors .dtors)
  }
  ASSERT(SIZEOF(.forbidden)==0,"Native XIP cannot contain mutable/implicit state")
  ASSERT(. <= 0xa06fe000,"Native module exceeds the original 7MiB FW slot")
  /DISCARD/ : { *(.comment .note* .eh_frame* .riscv.attributes) }
}
PROVIDE(snprintf = 0xa002d7fc);
