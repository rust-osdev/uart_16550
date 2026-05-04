# GNU Assembler Syntax (GAS) with AT&T Assembly Flavor
#
# This is understood by LLVM.

.extern rust_entry

.code32

.section .multiboot_header, "a", @progbits

.align 8
.global multiboot_header
multiboot_header:
    .long   0x1badb002
    .long   0x0
    .long  -0x1badb002

.section .text

.global start
start:
    mov     $stack_end, %esp
    mov     $stack_end, %ebp

    mov     $0x42  , %eax
    mov     $0x1337, %ebx
    mov     $0xc0ffee, %ecx

    push    %ecx
    push    %ebx
    push    %eax
    call    rust_entry
    ud2

.section .data

.align 16
stack_begin:
    .zero 16384
stack_end:
