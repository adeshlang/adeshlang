#!/usr/bin/env python3
"""
Generate a complete WASM module for string testing with memory management.

Exports:
- memory: 1 page (64KB) of linear memory
- malloc(size: i32) -> i32: Bump allocator starting at offset 1024
- free(ptr: i32): No-op
- get_len(ptr: i32) -> i32: Returns strlen (count until null byte)

The module uses a global variable at offset 0-3 to track the next allocation pointer.
"""

def leb128_unsigned(n):
    """Encode unsigned integer as LEB128"""
    result = []
    while True:
        byte = n & 0x7f
        n >>= 7
        if n == 0:
            result.append(byte)
            break
        result.append(byte | 0x80)
    return bytes(result)

def make_section(section_id, content):
    """Create a WASM section with id and content"""
    return bytes([section_id]) + leb128_unsigned(len(content)) + content

def make_vector(items):
    """Create a WASM vector (count + items)"""
    return leb128_unsigned(len(items)) + b''.join(items)

def make_string(s):
    """Create a WASM string (length + bytes)"""
    encoded = s.encode('utf-8')
    return leb128_unsigned(len(encoded)) + encoded

# Build the module piece by piece

# === Type Section ===
# Type 0: (i32) -> i32  (malloc, get_len)
# Type 1: (i32) -> ()   (free)
type_section = make_section(0x01, make_vector([
    bytes([0x60, 0x01, 0x7f, 0x01, 0x7f]),  # (i32) -> i32
    bytes([0x60, 0x01, 0x7f, 0x00]),         # (i32) -> ()
]))

# === Function Section ===
# func 0: type 0 (malloc)
# func 1: type 1 (free)
# func 2: type 0 (get_len)
func_section = make_section(0x03, make_vector([
    bytes([0x00]),  # malloc: type 0
    bytes([0x01]),  # free: type 1
    bytes([0x00]),  # get_len: type 0
]))

# === Memory Section ===
# 1 memory, min 1 page, no max
memory_section = make_section(0x05, make_vector([
    bytes([0x00, 0x01]),  # limits: min=1, no max
]))

# === Global Section ===
# 1 global: i32 mutable, init to 1024 (first allocation offset)
global_section = make_section(0x06, make_vector([
    bytes([0x7f, 0x01, 0x41, 0x80, 0x08, 0x0b]),  # i32 mut, i32.const 1024, end
]))

# === Export Section ===
exports = [
    make_string("memory") + bytes([0x02, 0x00]),   # memory 0
    make_string("malloc") + bytes([0x00, 0x00]),   # func 0
    make_string("free") + bytes([0x00, 0x01]),     # func 1
    make_string("get_len") + bytes([0x00, 0x02]), # func 2
]
export_section = make_section(0x07, make_vector(exports))

# === Code Section ===

# Function 0: malloc(size) -> ptr
# Returns current alloc_ptr, then advances it by size
# Code: global.get 0, global.get 0, local.get 0, i32.add, global.set 0, end
malloc_body = bytes([
    0x00,        # 0 locals
    0x23, 0x00,  # global.get 0 (return value)
    0x23, 0x00,  # global.get 0
    0x20, 0x00,  # local.get 0 (size)
    0x6a,        # i32.add
    0x24, 0x00,  # global.set 0
    0x0b,        # end
])

# Function 1: free(ptr) -> ()
# No-op
free_body = bytes([
    0x00,  # 0 locals
    0x0b,  # end
])

# Function 2: get_len(ptr) -> len
# Loop: load byte at ptr+i, if 0 return i, else i++
# Code with 1 local (i: i32)
get_len_body = bytes([
    0x01, 0x01, 0x7f,  # 1 local of type i32 (index i)
    
    # block $done
    0x02, 0x40,
    
    # loop $loop
    0x03, 0x40,
    
    # load byte: i32.load8_u (ptr + i)
    0x20, 0x00,        # local.get 0 (ptr)
    0x20, 0x01,        # local.get 1 (i)
    0x6a,              # i32.add
    0x2d, 0x00, 0x00,  # i32.load8_u align=0 offset=0
    
    # if byte == 0, break
    0x45,        # i32.eqz
    0x0d, 0x01,  # br_if 1 (break to $done block)
    
    # i = i + 1
    0x20, 0x01,  # local.get 1
    0x41, 0x01,  # i32.const 1
    0x6a,        # i32.add
    0x21, 0x01,  # local.set 1
    
    # continue loop
    0x0c, 0x00,  # br 0
    
    0x0b,  # end loop
    0x0b,  # end block
    
    # return i
    0x20, 0x01,  # local.get 1
    0x0b,        # end func
])

# Wrap bodies with size prefix
def make_func_body(body):
    return leb128_unsigned(len(body)) + body

code_section = make_section(0x0a, make_vector([
    make_func_body(malloc_body),
    make_func_body(free_body),
    make_func_body(get_len_body),
]))

# === Assemble Module ===
wasm_module = (
    b'\x00asm\x01\x00\x00\x00' +  # magic + version
    type_section +
    func_section +
    memory_section +
    global_section +
    export_section +
    code_section
)

with open('examples/wasm/string.wasm', 'wb') as f:
    f.write(wasm_module)

print(f"Created examples/wasm/string.wasm ({len(wasm_module)} bytes)")

# Verify by printing hex
print("Hex dump:")
print(' '.join(f'{b:02x}' for b in wasm_module[:64]) + " ...")
