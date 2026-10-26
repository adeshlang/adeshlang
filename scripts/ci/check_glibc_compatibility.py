#!/usr/bin/env python3
"""
Verify ELF binaries against a maximum allowed GLIBC symbol version baseline.
Usage: python3 check_glibc_compatibility.py <max_glibc_version> <file_or_dir>...
Example: python3 check_glibc_compatibility.py 2.35 ./dist/linux-x86_64/bin
"""

import sys
import os
import subprocess
import re

def parse_version(ver_str):
    try:
        parts = [int(p) for p in ver_str.split(".")]
        return tuple(parts)
    except Exception:
        return (0,)

def check_file(filepath, max_version, max_ver_tuple):
    # Check if file is ELF
    try:
        with open(filepath, "rb") as f:
            magic = f.read(4)
            if magic != b"\x7fELF":
                return True, []
    except Exception:
        return True, []

    cmd = ["readelf", "-sW", filepath]
    try:
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=True)
    except Exception as e:
        return False, [f"Failed to run readelf on {filepath}: {e}"]

    violations = []
    # Pattern to match GLIBC_2.xx in undefined dynamic symbols
    # e.g.: 159: 0000000000000000     0 FUNC    WEAK   DEFAULT  UND pidfd_spawnp@GLIBC_2.39 (26)
    # or:   ... FUNC    GLOBAL DEFAULT  UND foo@GLIBC_2.38
    pattern = re.compile(r'\bUND\s+([^\s@]+)@GLIBC_([0-9]+\.[0-9]+(?:\.[0-9]+)?)')

    for line in proc.stdout.splitlines():
        if "UND" not in line or "@GLIBC_" not in line:
            continue
        m = pattern.search(line)
        if m:
            sym_name = m.group(1)
            ver_str = m.group(2)
            sym_ver_tuple = parse_version(ver_str)
            if sym_ver_tuple > max_ver_tuple:
                violations.append((sym_name, ver_str, line.strip()))

    if violations:
        return False, violations
    return True, []

def main():
    if len(sys.argv) < 3:
        print("Usage: check_glibc_compatibility.py <max_glibc_version> <file_or_dir>...")
        sys.exit(1)

    max_version = sys.argv[1]
    max_ver_tuple = parse_version(max_version)
    targets = sys.argv[2:]

    total_checked = 0
    failures = 0

    print(f"==================================================")
    print(f"GLIBC Compatibility Gate: Max Allowed = GLIBC_{max_version}")
    print(f"==================================================")

    for target in targets:
        if os.path.isfile(target):
            files = [target]
        elif os.path.isdir(target):
            files = []
            for root, _, filenames in os.walk(target):
                for fn in filenames:
                    files.append(os.path.join(root, fn))
        else:
            print(f"Warning: path not found: {target}")
            continue

        for fpath in files:
            ok, violations = check_file(fpath, max_version, max_ver_tuple)
            if violations and not ok:
                failures += 1
                print(f"\n❌ VIOLATION in {fpath}:")
                for item in violations:
                    if isinstance(item, tuple):
                        sym, ver, line = item
                        print(f"   Symbol `{sym}` requires GLIBC_{ver} > GLIBC_{max_version}")
                        print(f"     Details: {line}")
                    else:
                        print(f"   {item}")
            elif ok and open(fpath, "rb").read(4) == b"\x7fELF":
                total_checked += 1
                print(f"✓ {fpath} passes GLIBC_{max_version} baseline check")

    print(f"\nChecked {total_checked} ELF binary/binaries.")
    if failures > 0:
        print(f"❌ Gate failed: {failures} binary/binaries violated the GLIBC_{max_version} compatibility baseline!")
        sys.exit(1)
    else:
        print(f"✅ All binaries satisfy the GLIBC_{max_version} compatibility contract.")
        sys.exit(0)

if __name__ == "__main__":
    main()
