import re
import sys

src = open('linker/tests/common/wasm_fixtures.rs').read()
consts = {}
for m in re.finditer(r'pub const (\w+): &str = "(.*?)"\s*;', src, re.S):
    consts[m.group(1)] = re.sub(r'[\s\\]', '', m.group(2))

new = {
    'PURE_CALLEE_OBJECT_HEX': 'twice',
    'PURE_CALLER_OBJECT_HEX': 'add',
    'DATA_OWNER_OBJECT_HEX': 'data',
    'DATA_USER_OBJECT_HEX': 'use',
    'WASI_ENTRY_OBJECT_HEX': 'start',
    'WASI_GREET_OBJECT_HEX': 'greet',
    'PLAIN_OBJECT_HEX': 'hello_plain',
    'EXPORTED_OBJECT_HEX': 'hello_exported',
}
print('consts found:', len(consts))
bad = 0
for name, obj in new.items():
    ref = open('.scratch/wasmfix/%s.o' % obj, 'rb').read().hex() if not obj.startswith('hello') else None
    got = consts.get(name, '')
    if ref is None:
        print(name, len(got) // 2, 'bytes (no reference object)')
        continue
    ok = got == ref
    bad += 0 if ok else 1
    print(name, 'MATCH' if ok else 'MISMATCH ref=%d got=%d' % (len(ref), len(got)))
sys.exit(1 if bad else 0)
