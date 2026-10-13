/* Replaces only mv a0,s4; jal lv_obj_create in exact stock 0031.
 * Package rejection and normally returning callbacks preserve the original
 * result/continuation; native exceptions or hangs cannot be isolated here. */
    .option norelax
    .section .text.entry,"ax",@progbits
    .balign 4
    .global ap01_entry
    .type ap01_entry,@function
ap01_entry:
    addi sp,sp,-16
    sw ra,12(sp)
    sw s0,8(sp)
    mv a0,s4
    call stock_lv_obj_create
    mv s0,a0
    beqz a0,2f
    call ap01_boot_run
    call ap01_emit_result
    j 1f
2:  li a0,1
    call ap01_emit_result
1:  mv a0,s0
    lw s0,8(sp)
    lw ra,12(sp)
    addi sp,sp,16
    ret
    .size ap01_entry,.-ap01_entry
