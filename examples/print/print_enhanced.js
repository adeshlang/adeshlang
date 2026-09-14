
(async function(){
  const wasmFile = 'print_enhanced.wasm';
  let bytes;
  const isNode = (typeof process !== 'undefined' && process.versions && process.versions.node);
  if (isNode) {
    const fs = require('fs');
    const path = require('path');
    const p = path.join(__dirname, wasmFile);
    bytes = fs.readFileSync(p);
  } else {
    const resp = await fetch(wasmFile);
    bytes = await resp.arrayBuffer();
  }
  let memory = new WebAssembly.Memory({initial:1});
  let heap = 2048;
  const imports = {
    env: {
      print_f64: (x) => { if (isNode) { process.stdout.write(String(x)); } else { console.log(String(x)); } },
      print_str: (ptr, len) => {
        const buf = new Uint8Array(memory.buffer, ptr, len);
        const s = new TextDecoder('utf-8').decode(buf);
        if (isNode) { process.stdout.write(s); } else { console.log(s); }
      },
      alloc: (len) => { const p = heap; heap += len; return p; },
      concat2: (p1,l1,p2,l2) => {
        const out = heap; heap += (l1+l2);
        new Uint8Array(memory.buffer, out, l1).set(new Uint8Array(memory.buffer, p1, l1));
        new Uint8Array(memory.buffer, out+l1, l2).set(new Uint8Array(memory.buffer, p2, l2));
        return out;
      }
    }
  };
  const { instance } = await WebAssembly.instantiate(bytes, imports);
  memory = instance.exports.memory;
  instance.exports.main();
})();