ENTRY(start)

PHDRS
{
    kernel_rx PT_LOAD FLAGS(5);
    kernel_rw PT_LOAD FLAGS(6);
    kernel_ro PT_LOAD FLAGS(4);
}

SECTIONS
{
    /*
     * QEMU's RISC-V virt board maps RAM starting at 0x8000_0000.
     * Load the guest image slightly above that base.
     */
    .text 0x80200000 : AT(0x80200000) ALIGN(4K)
    {
        *(.text.start)
        *(.text .text.*)
    } : kernel_rx

    .rodata :
    {
        *(.rodata .rodata.*)
    } : kernel_ro

    .data :
    {
        *(.data .data.*)
    } : kernel_rw

    .bss :
    {
        *(COMMON)
        *(.bss .bss.*)
    } : kernel_rw

    /DISCARD/ :
    {
        *(.comment .comment.*)
        *(.dynamic)
        *(.eh_frame*)
        *(.got .got.*)
        *(.note*)
        *(.plt .plt.*)
        *(.rela .rela.*)
    }
}
