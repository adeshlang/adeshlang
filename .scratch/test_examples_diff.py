import os
import subprocess
import sys

ADESH = os.path.abspath("target/debug/adesh.exe")

def run_file(file_path):
    # Run with interpreter
    try:
        interp = subprocess.run(
            [ADESH, "run", "--quiet", "--interpreter", file_path],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=4
        )
        interp_res = (interp.returncode, interp.stdout, interp.stderr)
    except subprocess.TimeoutExpired:
        interp_res = (-999, "", "TIMEOUT")
    except Exception as e:
        interp_res = (-998, "", str(e))

    # Run with Adesh Native JIT
    try:
        njit = subprocess.run(
            [ADESH, "run", "--quiet", "--njit", "--jit-backend=adesh", file_path],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=4
        )
        njit_res = (njit.returncode, njit.stdout, njit.stderr)
    except subprocess.TimeoutExpired:
        njit_res = (-999, "", "TIMEOUT")
    except Exception as e:
        njit_res = (-998, "", str(e))

    return interp_res, njit_res

def test_directory(d):
    if not os.path.isdir(d):
        return
    files = [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".adesh") and not f.endswith(".test.adesh")]
    print(f"\n=== Testing {d} ({len(files)} files) ===", flush=True)
    for f in files:
        interp_res, njit_res = run_file(f)
        i_code, i_out, i_err = interp_res
        n_code, n_out, n_err = njit_res

        i_out = i_out.replace("\r\n", "\n").strip()
        n_out = n_out.replace("\r\n", "\n").strip()
        i_err = i_err.replace("\r\n", "\n").strip()
        n_err = n_err.replace("\r\n", "\n").strip()

        match = (i_code == n_code) and (i_out == n_out)
        if match:
            print(f"[OK] {os.path.basename(f)} (exit {i_code})", flush=True)
        else:
            print(f"[FAIL] {os.path.basename(f)}", flush=True)
            print(f"   Interp: exit={i_code} out={repr(i_out[:80])} err={repr(i_err[:80])}", flush=True)
            print(f"   NJIT:   exit={n_code} out={repr(n_out[:80])} err={repr(n_err[:80])}", flush=True)

if __name__ == "__main__":
    dirs = sys.argv[1:] if len(sys.argv) > 1 else [
        "examples/01_Basics",
        "examples/basics",
        "examples/conditionals",
        "examples/loops",
        "examples/functions",
        "examples/fib",
        "examples/print",
        "examples/math",
        "examples/syntax",
        "examples/structures",
        "examples/arrays",
        "examples/unsafe",
    ]
    for d in dirs:
        test_directory(d)
