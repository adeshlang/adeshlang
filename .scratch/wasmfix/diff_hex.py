import re

src = open('linker/tests/common/wasm_fixtures.rs').read()
consts = {
    m.group(1): re.sub(r'[\s\\]', '', m.group(2))
    for m in re.finditer(r'pub const (\w+): &str = "(.*?)"\s*;', src, re.S)
}
for name, obj in [
    ('PURE_CALLER_OBJECT_HEX', 'add'),
    ('DATA_USER_OBJECT_HEX', 'use'),
    ('WASI_ENTRY_OBJECT_HEX', 'start'),
    ('WASI_GREET_OBJECT_HEX', 'greet'),
]:
    ref = open('.scratch/wasmfix/%s.o' % obj, 'rb').read().hex()
    got = consts[name]
    i = next((k for k in range(min(len(ref), len(got))) if ref[k] != got[k]), min(len(ref), len(got)))
    print('%s diverges at hex %d (byte %d) ref_len=%d got_len=%d' % (name, i, i // 2, len(ref), len(got)))
    print('  ref:', ref[max(0, i - 10):i + 40])
    print('  got:', got[max(0, i - 10):i + 40])
