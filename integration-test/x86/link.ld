/* Set entry point to symbol from start.S. */
ENTRY(start)

PHDRS
{
    kernel_rx   PT_LOAD FLAGS(5);
    kernel_rw   PT_LOAD FLAGS(6);
    kernel_ro   PT_LOAD FLAGS(4);
    note        PT_NOTE;
}

SECTIONS
{
    /*
     * Physical LOAD address 8M doesn't clash with firmware or MMIO in QEMU.
     */
    .text 8M : AT(8M) ALIGN(4K)
    {
        /*
         * Multiboot (v1) header must be within the first 8192 byte of the ELF.
         */
        KEEP(*(.multiboot_header));
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

    .note.xen_pvh : {
        *(.note.xen_pvh)
    } : note

    /DISCARD/ :
    {
      *(.comment .comment.*)
      *(.dynamic)
      *(.eh_frame*)
      *(.got .got.*)
      *(.note.*)
      *(.plt .plt.*)
      *(.rela .rela.*)
    }
}
