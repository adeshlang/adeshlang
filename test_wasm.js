const fs = require('fs');

async function run() {
    console.log('=== Testing Adesh-generated WASM with Node.js ===');
    const wasmBuffer = fs.readFileSync('test_simple.wasm');
    console.log(`Read ${wasmBuffer.length} bytes from test_simple.wasm`);

    const wasmModule = await WebAssembly.instantiate(wasmBuffer, {});
    const { instance } = wasmModule;

    console.log('Exported symbols:', Object.keys(instance.exports));

    if (typeof instance.exports._start === 'function') {
        console.log('Invoking instance.exports._start()...');
        const resStart = instance.exports._start();
        console.log('Result from _start():', resStart);
    }

    if (typeof instance.exports.main === 'function') {
        console.log('Invoking instance.exports.main()...');
        const resMain = instance.exports.main();
        console.log('Result from main():', resMain);
    }

    console.log('=== WebAssembly Execution Finished Successfully ===');
}

run().catch(err => {
    console.error('Error running WebAssembly module:', err);
    process.exit(1);
});
