import re

NEW = [
    ('INDIRECT_CALL_OBJECT_HEX', 'fptrs'),
    ('TABLE_DATA_OBJECT_HEX', 'vtab'),
    ('TABLE_CALLER_OBJECT_HEX', 'caller'),
    ('TABLE_CALLEE_OBJECT_HEX', 'fns'),
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
    src = src[: match.start()] + block + src[match.end():]
open(path, 'w', newline='').write(src)
print('rewrote %d fixtures' % len(NEW))
