import subprocess
import os
import sys

examples_dir = "examples/structures"
files = [
    "basic_struct.adesh",
    "dynamic_types.adesh",
    "ecommerce_example.adesh",
    "main.adesh",
    "mixed_types.adesh",
    "nested_struct.adesh",
    "struct_collections.adesh",
    "struct_fields.adesh",
    "struct_functions.adesh",
    "struct_optional_suffix_ok.adesh",
    "struct_strict_invalid.adesh",
    "struct_strict_overflow.adesh",
    "struct_validation.adesh",
]

backends = [
    ("interpreter", ["target\\debug\\adesh.exe", "run"]),
    ("jit", ["target\\debug\\adesh.exe", "run", "--jit"]),
    ("njit", ["target\\debug\\adesh.exe", "run", "--njit"]),
    ("vm", ["target\\debug\\adesh.exe", "run", "--vm"]),
    ("aot", ["target\\debug\\adesh.exe", "run", "--aot"]),
    ("wasm", ["target\\debug\\adesh.exe", "run", "--backend=wasm"]),
]

results = {}

for f in files:
    filepath = os.path.join(examples_dir, f)
    print(f"\n======================================", flush=True)
    print(f"Testing {f}", flush=True)
    print(f"======================================", flush=True)
    results[f] = {}
    for b_name, cmd_base in backends:
        cmd = cmd_base + [filepath]
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, timeout=12)
            stdout_lines = [l for l in p.stdout.splitlines() if not l.strip().startswith("✓") and not l.strip().startswith("✗") and not l.strip().startswith("⚠") and not l.strip().startswith("Warning:") and not l.strip().startswith("Successfully built")]
            stdout_clean = "\n".join(stdout_lines).strip()
            stderr_clean = p.stderr.strip()
            results[f][b_name] = {
                "code": p.returncode,
                "stdout": stdout_clean,
                "stderr": stderr_clean,
            }
            status_str = f"CODE {p.returncode}" if p.returncode != 0 else "OK"
            print(f"  {b_name:12}: {status_str}", flush=True)
            if p.returncode != 0:
                err_preview = (stderr_clean or stdout_clean).replace("\n", " ")[:120]
                print(f"    err: {err_preview}", flush=True)
        except subprocess.TimeoutExpired:
            results[f][b_name] = {"code": -999, "stdout": "", "stderr": "TIMEOUT"}
            print(f"  {b_name:12}: TIMEOUT", flush=True)
        except Exception as e:
            results[f][b_name] = {"code": -1, "stdout": "", "stderr": str(e)}
            print(f"  {b_name:12}: EXCEPTION {e}", flush=True)

# Compare outputs across backends
print("\n\n" + "="*80, flush=True)
print("COMPARISON SUMMARY", flush=True)
print("="*80, flush=True)

for f in files:
    print(f"\n--- File: {f} ---", flush=True)
    interp = results[f].get("interpreter", {})
    interp_out = interp.get("stdout", "")
    interp_code = interp.get("code", 0)
    print(f"  [interpreter] code={interp_code}, out_len={len(interp_out)}", flush=True)
    if interp_code != 0:
        print(f"    interpreter err: {interp.get('stderr','')[:120]}", flush=True)
            
    for b_name, _ in backends[1:]:
        b_res = results[f].get(b_name, {})
        b_out = b_res.get("stdout", "")
        b_code = b_res.get("code", 0)
        
        matches = (b_code == interp_code) and (b_out == interp_out)
        if matches:
            print(f"  [{b_name:11}] MATCHES interpreter", flush=True)
        else:
            print(f"  [{b_name:11}] MISMATCH: code={b_code} (vs {interp_code})", flush=True)
            if b_out != interp_out:
                b_lines = b_out.splitlines()
                last_line = b_lines[-1] if b_lines else "<none>"
                interp_lines = interp_out.splitlines()
                interp_last = interp_lines[-1] if interp_lines else "<none>"
                print(f"    stdout mismatch! b_lines={len(b_lines)} vs interp_lines={len(interp_lines)}", flush=True)
                print(f"    interp_last: {interp_last}", flush=True)
                print(f"    {b_name}_last:   {last_line}", flush=True)
                if b_code != 0:
                    print(f"    stderr: {b_res.get('stderr','')[:120]}", flush=True)
