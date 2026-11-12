// A real GNU assembler input. No _start: adeshlink must supply it.
.text
.global main
.type main, %function
main:
    mov x9, sp
    tst x9, #15
    b.ne bad_alignment
    ldr x10, [sp]
    cmp x0, x10
    b.ne bad_argc
    cbz x0, bad_argc
    add x10, sp, #8
    cmp x1, x10
    b.ne bad_argv

    // Derive envp by walking argv to its null terminator, independently
    // of the arithmetic used by the linker's startup sequence.
    mov x11, x1
next_arg:
    ldr x12, [x11], #8
    cbnz x12, next_arg
    cmp x2, x11
    b.ne bad_envp

    // The QEMU command supplies this sentinel. Validate the pointed-to
    // environment as well as its address.
    mov x13, x2
next_env:
    ldr x14, [x13], #8
    cbz x14, bad_envp
    adrp x15, sentinel
    add x15, x15, :lo12:sentinel
compare_env:
    ldrb w16, [x14], #1
    ldrb w17, [x15], #1
    cmp w16, w17
    b.ne next_env
    cbnz w16, compare_env

    mov x0, #1
    adrp x1, message
    add x1, x1, :lo12:message
    mov x2, #message_length
    mov x8, #64
    svc #0
    cmp x0, #message_length
    b.ne bad_write
    mov x0, #37
    ret
bad_alignment:
    mov x0, #81
    ret
bad_argc:
    mov x0, #82
    ret
bad_argv:
    mov x0, #83
    ret
bad_envp:
    mov x0, #84
    ret
bad_write:
    mov x0, #85
    ret
.size main, .-main

.section .rodata
sentinel:
    .asciz "ADESH_STARTUP_SENTINEL=ready"
message:
    .ascii "aarch64 startup ok\n"
.set message_length, .-message
.section .note.GNU-stack,"",%progbits
