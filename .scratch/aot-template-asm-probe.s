# ------------------------------------------------------------------
# Adesh Generated x86_64 Assembly: aot-template-asm-probe
# Zero-dependency Native Machine Code Generator
# ------------------------------------------------------------------
    .intel_syntax noprefix

    .section .text
    .align 16

    .globl main
    .type main, @function
main:
    push rbp
    mov rbp, rsp
    mov rcx, 42
    sub rsp, 32
    call aot_make_i64
    add rsp, 32
    mov rcx, rax
    sub rsp, 32
    call aot_to_string
    add rsp, 32
    mov rcx, rax
    xor rdx, rdx
    sub rsp, 32
    call aot_print_string
    add rsp, 32
    sub rsp, 32
    call aot_print_newline
    add rsp, 32
    xor rax, rax
    mov rsp, rbp
    pop rbp
    ret
    .size main, .-main

