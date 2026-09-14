#!/usr/bin/env python3
"""
Generate a complete WASM module for array testing with memory management.

Exports:
- memory: 1 page (64KB) of linear memory
- malloc(size: i32) -> i32: Bump allocator starting at offset 1024
- free(ptr: i32): No-op
- sum_array(ptr: i32, len: i32) -> i64: Sum all i64 elements
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

# === Type Section ===
# Type 0: (i32) -> i32  (malloc)
# Type 1: (i32) -> ()   (free)
# Type 2: (i32, i32) -> i64  (sum_array)
type_section = make_section(0x01, make_vector([
    bytes([0x60, 0x01, 0x7f, 0x01, 0x7f]),         # (i32) -> i32
    bytes([0x60, 0x01, 0x7f, 0x00]),               # (i32) -> ()
    bytes([0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7e]),  # (i32, i32) -> i64
]))

# === Function Section ===
func_section = make_section(0x03, make_vector([
    bytes([0x00]),  # malloc: type 0
    bytes([0x01]),  # free: type 1
    bytes([0x02]),  # sum_array: type 2
]))

# === Memory Section ===
memory_section = make_section(0x05, make_vector([
    bytes([0x00, 0x01]),  # limits: min=1, no max
]))

# === Global Section ===
global_section = make_section(0x06, make_vector([
    bytes([0x7f, 0x01, 0x41, 0x80, 0x08, 0x0b]),  # i32 mut, i32.const 1024, end
]))

# === Export Section ===
exports = [
    make_string("memory") + bytes([0x02, 0x00]),
    make_string("malloc") + bytes([0x00, 0x00]),
    make_string("free") + bytes([0x00, 0x01]),
    make_string("sum_array") + bytes([0x00, 0x02]),
]
export_section = make_section(0x07, make_vector(exports))

# === Code Section ===

# Function 0: malloc(size) -> ptr
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
free_body = bytes([
    0x00,  # 0 locals
    0x0b,  # end
])

# Function 2: sum_array(ptr, len) -> i64
# Sums len i64 elements starting at ptr
# locals: idx (i32, local 2), sum (i64, local 3)
# params: ptr (i32, local 0), len (i32, local 1)
sum_array_body = bytes([
    0x02, 0x01, 0x7f, 0x01, 0x7e,  # 2 locals: 1 i32 (idx), 1 i64 (sum)
    
    # Loop over array
    # block $break (label 1)
    0x02, 0x40,
    # loop $continue (label 0)
    0x03, 0x40,
    
    # if idx == len, break
    0x20, 0x02,  # local.get 2 (idx)
    0x20, 0x01,  # local.get 1 (len)
    0x46,        # i32.eq
    0x0d, 0x01,  # br_if 1 (break)
    
    # sum = sum + i64.load(ptr + idx * 8)
    0x20, 0x03,  # local.get 3 (sum)
    
    0x20, 0x00,  # local.get 0 (ptr)
    0x20, 0x02,  # local.get 2 (idx)
    0x41, 0x03,  # i32.const 3 (shift left by 3 = mul 8)
    0x74,        # i32.shl
    0x6a,        # i32.add
    
    0x29, 0x00, 0x00,  # i64.load align=0 offset=0
    0x7c,        # i64.add
    0x21, 0x03,  # local.set 3 (sum)
    
    # idx++
    0x20, 0x02,  # local.get 2
    0x41, 0x01,  # i32.const 1
    0x6a,        # i32.add
    0x21, 0x02,  # local.set 2
    
    # branch to loop
    0x0c, 0x00,  # br 0
    
    0x0b,  # end loop
    0x0b,  # end block
    
    # return sum
    0x20, 0x03,  # local.get 3
    0x0b,        # end func
])

def make_func_body(body):
    return leb128_unsigned(len(body)) + body

code_section = make_section(0x0a, make_vector([
    make_func_body(malloc_body),
    make_func_body(free_body),
    make_func_body(sum_array_body),
]))

# === Assemble Module ===
wasm_module = (
    b'\x00asm\x01\x00\x00\x00' +
    type_section +
    func_section +
    memory_section +
    global_section +
    export_section +
    code_section
)

with open('examples/wasm/array.wasm', 'wb') as f:
    f.write(wasm_module)

print(f"Created examples/wasm/array.wasm ({len(wasm_module)} bytes)")
