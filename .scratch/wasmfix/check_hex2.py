import re

src = open('linker/tests/common/wasm_fixtures.rs').read()
pairs = [
    ('INDIRECT_CALL_OBJECT_HEX', 'fptrs'),
    ('TABLE_DATA_OBJECT_HEX', 'vtab'),
    ('TABLE_CALLER_OBJECT_HEX', 'caller'),
    ('TABLE_CALLEE_OBJECT_HEX', 'fns'),
]
for name, obj in pairs:
    m = re.search(r'pub const %s: &str = "(.*?)";' % name, src, re.S)
    assert m, name
    hexs = ''.join(m.group(1).split())
    raw = open('.scratch/wasmfix/%s.o' % obj, 'rb').read().hex()
    print(name, 'MATCH' if hexs == raw else 'MISMATCH', len(hexs) // 2, len(raw) // 2)
