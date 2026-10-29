import re

NEW = [
    ('PURE_CALLEE_OBJECT_HEX', 'twice'),
    ('PURE_CALLER_OBJECT_HEX', 'add'),
    ('DATA_OWNER_OBJECT_HEX', 'data'),
    ('DATA_USER_OBJECT_HEX', 'use'),
    ('WASI_ENTRY_OBJECT_HEX', 'start'),
    ('WASI_GREET_OBJECT_HEX', 'greet'),
]

path = 'linker/tests/common/wasm_fixtures.rs'
src = open(path, newline='').read()
for name, obj in NEW:
    raw = open('.scratch/wasmfix/%s.o' % obj, 'rb').read().hex()
    lines = [raw[i:i + 100] for i in range(0, len(raw), 100)]
    block = 'pub const %s: &str = "\\\n%s";' % (name, '\n'.join(lines))
    pattern = re.compile(r'pub const %s: &str = ".*?";' % name, re.S)
    match = pattern.search(src)
    assert match, name
    assert '\n\n' not in block
    src = src[: match.start()] + block + src[match.end():]
open(path, 'w', newline='').write(src)
print('rewrote %d fixtures' % len(NEW))
