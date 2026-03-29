# GNU Assembler Syntax (GAS) with AT&T Assembly Flavor
#
# This is understood by LLVM.

# Symbol from Rust
.extern rust_entry

.code32

.section .multiboot_header, "a", @progbits

/*
 * Multiboot v1 Header.
 * Required so that we can be booted by QEMU via the "-kernel" parameter.
 */
.align 8
.global multiboot_header
multiboot_header:
    .long   0x1badb002
    .long   0x0
    .long  -0x1badb002

.section .text

.global start
start:
    # Set-up stack (stack grows downwards)
    mov     $stack_end, %esp
    mov     $stack_end, %ebp

    # Pass some parameters to the kernel
    mov     $0x42  , %eax
    mov     $0x1337, %ebx
    mov     $0xc0ffee, %ecx

    # x86 (i386) calling convention:
    # push arguments on stack in reverse order
    push    %ecx # 3rd parameter
    push    %ebx # 2nd parameter
    push    %eax # 1st parameter
    call    rust_entry
    ud2

.section .data

# 16-byte aligned 16k stack.
.align 16
stack_begin:
    .zero 16384
stack_end:
