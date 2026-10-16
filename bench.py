import difflib
import re
import subprocess
import time
from pathlib import Path

EXAMPLES_DIR = r"D:\Projects\AdeshLang\examples\print"
MODES = ["interpreter", "jit", "njit"]

def remove_ansi_codes(text: str) -> str:
    """Removes terminal ANSI color and formatting codes."""
    return re.sub(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])', '', text)

def normalize_output(text: str) -> str:
    """Normalizes line endings, strips whitespace, and removes non-printable bytes."""
    # Strip ANSI color codes first
    text = remove_ansi_codes(text)
    # Convert Windows CRLF (\r\n) to Unix LF (\n)
    text = text.replace('\r\n', '\n').replace('\r', '\n')
    # Strip trailing whitespace on each line and drop empty edge lines
    lines = [line.rstrip() for line in text.split('\n')]
    return '\n'.join(lines).strip()

def run_benchmarks():
    dir_path = Path(EXAMPLES_DIR)
    if not dir_path.exists():
        print(f"Error: Path '{EXAMPLES_DIR}' does not exist.")
        return

    adesh_files = list(dir_path.glob("**/*.adesh"))
    if not adesh_files:
        print(f"No .adesh files found in '{EXAMPLES_DIR}'.")
        return

    print(f"Found {len(adesh_files)} file(s). Running comparisons...\n")

    for file in adesh_files:
        print("=" * 60)
        print(f" Testing: {file.name}")
        print("=" * 60)

        raw_outputs = {}
        normalized_outputs = {}

        for mode in MODES:
            cmd = ["adesh", "run", f"--{mode}", str(file)]
            
            start_time = time.perf_counter()
            # Capture stdout and stderr separately
            res = subprocess.run(
                cmd, 
                stdout=subprocess.PIPE, 
                stderr=subprocess.PIPE, 
                text=True, 
                encoding="utf-8", 
                errors="replace"
            )
            elapsed_ms = round((time.perf_counter() - start_time) * 1000, 2)

            # Prefer STDOUT, but capture STDERR if STDOUT is empty
            stdout_text = res.stdout if res.stdout else res.stderr
            
            raw_outputs[mode] = stdout_text
            normalized_outputs[mode] = normalize_output(stdout_text)

            print(f"  [{mode:<11}] Completed in {elapsed_ms:>6.2f} ms")

        # Compare normalized outputs against the baseline (interpreter)
        baseline_mode = MODES[0]
        baseline_text = normalized_outputs[baseline_mode]
        
        mismatches = []
        for mode in MODES[1:]:
            if normalized_outputs[mode] != baseline_text:
                mismatches.append(mode)

        if not mismatches:
            print("\n  ✅ RESULT: PERFECT MATCH ACROSS ALL MODES")
            print(f"  Sample output: {repr(baseline_text[:60])}")
        else:
            print(f"\n  ❌ RESULT: MISMATCH DETECTED IN MODE(S): {', '.join(mismatches)}")
            
            # Print detailed character/byte diff against baseline
            print("\n  --- DETAILED DIFF DIAGNOSTICS ---")
            for mode in mismatches:
                print(f"\n  Diff: {baseline_mode} (baseline) vs {mode}:")
                
                # Raw repr comparison to reveal hidden bytes like \r, \t, or \ufeff
                print(f"    Baseline repr : {repr(baseline_text)}")
                print(f"    {mode:<12} repr : {repr(normalized_outputs[mode])}")
                
                # Line-by-line unified diff
                diff = list(difflib.unified_diff(
                    baseline_text.splitlines(),
                    normalized_outputs[mode].splitlines(),
                    fromfile=f"mode-{baseline_mode}",
                    tofile=f"mode-{mode}",
                    lineterm=""
                ))
                for line in diff:
                    print(f"      {line}")

        print("\n")

if __name__ == "__main__":
    run_benchmarks()