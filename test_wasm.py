"""Python test runner for Adesh-generated WebAssembly binaries."""

import os
import struct
import subprocess
import sys


def parse_and_validate_wasm(wasm_path: str):
    print(f"=== [Python] Inspecting WebAssembly binary: {wasm_path} ===")
    with open(wasm_path, "rb") as f:
        data = f.read()

    print(f"Binary size: {len(data)} bytes")
    assert len(data) >= 8, "WASM file too short"

    # Magic number: \x00asm
    magic = data[:4]
    version = struct.unpack("<I", data[4:8])[0]
    print(f"Magic: {magic} (Valid: {magic == b'\x00asm'})")
    print(f"Version: {version} (Valid: {version == 1})")
    assert magic == b"\x00asm", "Invalid WASM magic"
    assert version == 1, "Invalid WASM version"

    # Iterate through sections
    idx = 8
    sections = []
    section_names = {
        1: "Type",
        2: "Import",
        3: "Function",
        4: "Table",
        5: "Memory",
        6: "Global",
        7: "Export",
        8: "Start",
        9: "Element",
        10: "Code",
        11: "Data",
        12: "DataCount",
    }

    while idx < len(data):
        sec_id = data[idx]
        idx += 1

        # Read LEB128 section size
        sec_len = 0
        shift = 0
        while True:
            byte = data[idx]
            idx += 1
            sec_len |= (byte & 0x7F) << shift
            if (byte & 0x80) == 0:
                break
            shift += 7

        sec_name = section_names.get(sec_id, f"Custom({sec_id})")
        payload = data[idx : idx + sec_len]
        idx += sec_len
        sections.append((sec_id, sec_name, len(payload)))
        print(f"  Section ID {sec_id:2d} ({sec_name:8s}): {len(payload)} bytes")

    return data


def run_wasm_in_python(wasm_path: str):
    parse_and_validate_wasm(wasm_path)

    print("\n=== [Python] Executing WebAssembly module via Node runtime bridge ===")
    js_code = f"""
    const fs = require('fs');
    const buf = fs.readFileSync('{wasm_path.replace(os.sep, "/")}');
    WebAssembly.instantiate(buf, {{}}).then(({{ instance }}) => {{
        console.log('Exported functions:', Object.keys(instance.exports));
        if (typeof instance.exports._start === 'function') {{
            console.log('Result of _start():', instance.exports._start());
        }}
        if (typeof instance.exports.main === 'function') {{
            console.log('Result of main():', instance.exports.main());
        }}
    }}).catch(err => {{
        console.error(err);
        process.exit(1);
    }});
    """
    res = subprocess.run(["node", "-e", js_code], capture_output=True, text=True)
    print(res.stdout)
    if res.stderr:
        print("Stderr:", res.stderr)
    assert res.returncode == 0, f"Execution failed with code {res.returncode}"
    print("=== [Python] WASM Execution & Validation Passed! ===")


if __name__ == "__main__":
    wasm_file = sys.argv[1] if len(sys.argv) > 1 else "test_simple.wasm"
    run_wasm_in_python(wasm_file)
