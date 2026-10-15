import os
import re
import subprocess
import time
from pathlib import Path

# Path to your .adesh files
EXAMPLES_DIR = r"D:\Projects\AdeshLang\examples\print"
MODES = ["interpreter", "jit", "njit"]

# Helper to remove ANSI color escape codes (e.g. \x1b[32m)
def strip_ansi(text: str) -> str:
    ansi_regex = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
    return ansi_regex.sub('', text)

def clean_output(text: str) -> str:
    cleaned = strip_ansi(text)
    # Standardize line endings and strip leading/trailing whitespace
    return "\n".join(line.strip() for line in cleaned.splitlines() if line.strip())

def run_benchmarks():
    dir_path = Path(EXAMPLES_DIR)
    if not dir_path.exists():
        print(f"Error: Path '{EXAMPLES_DIR}' does not exist.")
        return

    # Use recursive search in case files are in subfolders like examples/print
    adesh_files = list(dir_path.glob("**/*.adesh"))
    if not adesh_files:
        print(f"No .adesh files found in '{EXAMPLES_DIR}'.")
        return

    print(f"Found {len(adesh_files)} file(s). Starting benchmarks...\n")

    summary = []

    for file in adesh_files:
        print("=" * 60)
        print(f" Testing: {file.relative_to(dir_path)}")
        print("=" * 60)

        raw_outputs = {}
        cleaned_outputs = {}
        execution_times = {}

        for mode in MODES:
            cmd = ["adesh", "run", f"--{mode}", str(file)]
            
            start_time = time.perf_counter()
            # Capture stdout and stderr separately
            res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
            elapsed_ms = round((time.perf_counter() - start_time) * 1000, 2)

            raw_stdout = res.stdout
            cleaned_stdout = clean_output(raw_stdout)

            raw_outputs[mode] = raw_stdout
            cleaned_outputs[mode] = cleaned_stdout
            execution_times[mode] = f"{elapsed_ms} ms"

            print(f"  [{mode:<11}] {elapsed_ms:>7.2f} ms")
            if res.stderr:
                print(f"    └─ stderr: {res.stderr.strip()[:60]}")

        # Compare cleaned STDOUT across all modes
        first_mode = MODES[0]
        reference_output = cleaned_outputs[first_mode]
        
        all_match = True
        mismatched_modes = []

        for mode in MODES:
            if cleaned_outputs[mode] != reference_output:
                all_match = False
                mismatched_modes.append(mode)

        status = "MATCH" if all_match else f"MISMATCH ({', '.join(mismatched_modes)})"
        
        # Display output preview or detailed diff on mismatch
        if not all_match:
            print("\n  ⚠️ DETECTED OUTPUT DIFFERENCE:")
            for mode in MODES:
                preview = repr(cleaned_outputs[mode][:50])
                print(f"    - Mode {mode:<11}: {preview}")
        else:
            preview = repr(reference_output[:50]) if reference_output else "<empty>"
            print(f"\n  Output match preview: {preview}")

        summary_row = {
            "File": file.name,
            "Consistency": "MATCH" if all_match else "MISMATCH",
        }
        for mode in MODES:
            summary_row[mode] = execution_times[mode]
        summary.append(summary_row)

        print("\n")

    # Print clean summary table
    print("=" * 60)
    print(" BENCHMARK & CONSISTENCY SUMMARY")
    print("=" * 60)
    
    header = f"{'File Name':<25} | {'Status':<10} | " + " | ".join(f"{m:<10}" for m in MODES)
    print(header)
    print("-" * len(header))
    
    for row in summary:
        times_str = " | ".join(f"{row[m]:<10}" for m in MODES)
        print(f"{row['File']:<25} | {row['Consistency']:<10} | {times_str}")

if __name__ == "__main__":
    run_benchmarks()