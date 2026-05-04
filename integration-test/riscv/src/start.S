.section .text.start

.globl start
start:
    la sp, stack_end
    call rust_entry

1:
    wfi
    j 1b

.section .bss.stack, "aw", @nobits
.align 16
stack_begin:
    .zero 16384
stack_end:
