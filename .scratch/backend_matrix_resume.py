import subprocess, re, pathlib, sys, json

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXE = ROOT / "target/debug/adesh.exe"
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")

def run(file, backend):
    try:
        p = subprocess.run([str(EXE), "run", "--" + backend, str(file)],
                           cwd=ROOT, capture_output=True, timeout=45)
        out, err, code = p.stdout, p.stderr, p.returncode
    except subprocess.TimeoutExpired as e:
        out, err, code = e.stdout or b"", e.stderr or b"", "timeout"
    text = ANSI.sub("", (out + err).decode("utf-8", "replace")).replace("\r", "")
    lines = []
    for line in text.splitlines():
        line = line.strip()
        if not line or any(s in line for s in (
            "backend ready", "Interpreter ready", "Successfully built:", "adeshlink",
            "warning:", "WASM backend ready", "Native JIT ready")):
            continue
        lines.append(line)
    return code, lines, text

if __name__ == "__main__":
    files = [ROOT / p for p in sys.argv[1:]] or list((ROOT / "examples/syntax").glob("*.adesh"))
    report = []
    for file in files:
        ref_code, ref, _ = run(file, "interpreter")
        for backend in ("aot", "jit-native", "wasm"):
            code, got, raw = run(file, backend)
            # Wall-clock output changes between runs, but must remain present.
            ref_static = [s for s in ref if not s.startswith(("Date:", "Local getters:"))]
            got_static = [s for s in got if not s.startswith(("Date:", "Local getters:"))]
            diffs = [(i, a, b) for i, (a, b) in enumerate(
                __import__("itertools").zip_longest(got_static, ref_static)) if a != b]
            entry = dict(file=file.name, backend=backend, code=code, reference_code=ref_code,
                         differences=diffs, output=raw)
            report.append(entry)
            print(json.dumps({k:v for k,v in entry.items() if k != "output"}), flush=True)
    (ROOT / ".scratch/backend_matrix_resume.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
