# Security Policy

Thank you for helping keep AdeshLang safe. This document describes which versions receive security fixes, what security properties the language provides (and does not provide), and exactly how to report a vulnerability in a way that gets it fixed and disclosed responsibly.

- **Version of this policy applies to:** AdeshLang v0.3.0 (pre-1.0)
- **Reporting channel:** GitHub Security Advisories (private) — see [Reporting a Vulnerability](#reporting-a-vulnerability)
- **Last updated:** 2026-09-03

---

## Table of Contents

1. [Scope](#scope)
2. [Supported Versions](#supported-versions)
3. [Security Model](#security-model)
4. [Security Properties by Area](#security-properties-by-area)
5. [Known Limitations / Out of Scope](#known-limitations--out-of-scope)
6. [Reporting a Vulnerability](#reporting-a-vulnerability)
7. [Vulnerability Handling Workflow](#vulnerability-handling-workflow)
8. [Coordinated Disclosure](#coordinated-disclosure)
9. [Security Hardening Guide for Users](#security-hardening-guide-for-users)
10. [Security FAQ](#security-faq)
11. [Recognition](#recognition)
12. [Contact](#contact)

---

## Scope

This policy covers the **AdeshLang toolchain and runtime**, which includes:

| Component | In scope |
| :--- | :--- |
| Language core (`adeshlang` crate): lexer, parser, type checker, borrow checker, semantic passes | ✅ |
| Execution backends: interpreter, bytecode VM, JIT (Cranelift), AOT (LLVM/Cranelift) | ✅ |
| Standard library: `core`, `IO`, `Math`, `JSON`, `Crypto`, `Compression`, `HTTP`, `TLS`, `Process`, `Time`, `Regex`, and all modules under `src/stdlib` | ✅ |
| ADL package manager: dependency resolution, `adesh.lock`, registry integrity checks | ✅ |
| Language Server (`als`) and editor integrations | ✅ |
| `ATP` (Adesh Transport Protocol) implementation | ✅ |
| Build/install scripts and generated installers | ✅ |
| Example code in `examples/` and `demo_project/` | ❌ (out of scope — see below) |

**Important:**

- **Security-sensitive built-in modules are never a substitute for review.** Code in `examples/`, `demo_project/`, `testing/`, and documentation snippets is illustrative; do not treat it as a security reference implementation.
- **Third-party dependencies** (e.g. `rustls`, `wasmtime`, `cranelift`, cryptographic crates) are repaired through upstream advisories. If a vulnerability is caused by an upstream dependency, we track the upstream advisory and ship an updated dependency rather than issuing a custom patch.
- **Reports about misconfiguration or misuse of the language** (e.g. an application that runs with `--allow-internal-network` and gets SSRF'd) are generally documentation issues, not vulnerabilities in the toolchain — but we still want to hear about them; see [Reporting a Vulnerability](#reporting-a-vulnerability).

---

## Supported Versions

AdeshLang is a **pre-1.0** project (current series **0.3.x**). Until 1.0, the security support policy is deliberately simple: **only the latest released minor series receives security fixes.**

| Version | Supported | Notes |
| :--- | :--- | :--- |
| `0.3.x` (latest) | ✅ **Yes** | Security fixes ship here. Patch releases are cut as-needed for fixing vulnerabilities. |
| `0.2.x` and earlier | ❌ No | End of life. Users must upgrade to the latest release. |

| What changes on release | Detail |
| :--- | :--- |
| Fix path for vulnerabilities | A new patch/minor release in the `0.3` series. We do not backport to EOL series. |
| Timeframe | See [Vulnerability Handling Workflow](#vulnerability-handling-workflow) — same SLA for all supported releases. |
| How you know a fix shipped | A `SECURITY`/advisory note in the release notes and, when a CVE is assigned, the GitHub advisory is updated with the fixed version. |

> Once AdeshLang reaches **1.0**, this table will be expanded into a proper release-support window (e.g. latest minor + N-1 for a defined period, LTS designation policy, and a deprecation cycle). Until then, treat "upgrade to latest" as the only supported path.

---

## Security Model

AdeshLang is a statically-typed systems language with a **compile-time memory-safety model** at its core. The security model is layered:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 5: Application semantics (your code, your risk)        │
├─────────────────────────────────────────────────────────────┤
│ Layer 4: Runtime & standard library (TLS, HTTP, crypto,      │
│          Process, JSON, ...)                                 │
├─────────────────────────────────────────────────────────────┤
│ Layer 3: Execution backends (interpreter, VM, JIT, AOT,      │
│          WASM sandbox)                                       │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: Compiler & type system (type checker, borrow        │
│          checker, safety passes, constant-time validation)   │
├─────────────────────────────────────────────────────────────┤
│ Layer 1: Rust host (memory-safe host runtime, audited deps)  │
└─────────────────────────────────────────────────────────────┘
```

- **Layers 1–3** are what this policy protects: a bug that lets untrusted source code break out of the language's safety guarantees (memory corruption, type confusion, escape from the WASM sandbox, unauthorized host access through a builtin) is a vulnerability.
- **Layers 4–5** are where application security lives. A program using `Process.exec`, writing world-readable files, or calling `Crypto` primitives incorrectly can be insecure even though the toolchain itself is sound — those are *application* issues, not toolchain vulnerabilities, but see the [hardening guide](#security-hardening-guide-for-users).

### What the language guarantees

- **Memory safety by default:** no use-after-free, no double-free, no dangling pointers, no data races in safe code (enforced by the ownership/borrow checker and deterministic ARC with compile-time drop insertion).
- **Type safety:** expressions are checked before execution; untrusted source cannot falsify types in safe code.
- **Deterministic resource management:** deterministic drops and region-based allocation (`region`) close the "resource leak under attack" class of bugs.
- **Controlled escape hatches:** `unsafe` blocks, FFI (`extern`), raw pointers, `alloc`/`free`, and `share`/`strong`/`weak` reference types are explicit and localized, so reviewers can audit exactly where safety is delegated to the programmer.

### What the language does NOT guarantee

- It is **not** an operating-system sandbox by default. Code executed by `adesh run` has the same host privileges as any process you launch. Untrusted third-party code should be run in a VM, container, or the WASM backend (`wasmtime`), not in a bare interpreter process.
- Compile-time safety does not protect against **logic bugs, auth bypasses, or business-level flaws** in your programs.
- `unsafe`/FFI code is outside the safety guarantee; every `unsafe` block is a review point.

---

## Security Properties by Area

### 1. Memory & type safety (compiler, borrow checker)

- Ownership, borrowing, lifetimes, and drop insertion are enforced at compile time across all backends.
- Runtime type checks (arrays, bounds, casts) are generated for operations the static checker cannot prove safe.
- A bug report in this layer is **critical** by default (see [severity guidance](#how-to-write-a-good-report)) because it may enable memory corruption from safe code.

### 2. Networking & HTTP — SSRF protection

- The `HTTP` module blocks access to internal network ranges **by default**: `localhost`, `127.0.0.1`, `0.0.0.0`, `::1`, link-local (`169.254.`), and private ranges (`192.168.`, `10.`), plus the URL validation safeguards in `src/typesystem/safeguards/mod.rs`.
- This is designed to prevent **Server-Side Request Forgery (SSRF)** when untrusted input reaches an HTTP URL.
- The check can be disabled ONLY by compiling with the explicit feature flag:

  ```toml
  # Cargo.toml
  adeshlang = { features = ["allow_internal_network"] }
  ```

  ```bash
  # CLI build
  cargo build --features allow_internal_network
  ```

- Behave accordingly:

  - **Do not** ship production builds with `allow_internal_network`.
  - If you build with this flag, YOU are responsible for SSRF and network security of your application.
  - A report demonstrating that internal-network access is possible *without* this flag is a vulnerability; a report that it is possible *with* the flag is expected behavior.

### 3. Transport security (TLS)

- TLS is implemented with **`rustls`** — a memory-safe Rust TLS stack. OpenSSL is not used.
- TLS verification is enforced by default; certificate validation must not be disableable through the public API.
- Reports of trust-store bypass, hostname-verification bypass, or ALPN/protocol-confusion bugs are treated as high severity.

### 4. Cryptography (standard library)

The `Crypto` module is engineered for **secure defaults**; see `docs/FEATURES.md` for the full algorithm matrix. Key properties:

| Property | Implementation |
| :--- | :--- |
| Authenticated encryption by default | AES-256-GCM and ChaCha20-Poly1305 (AEAD) are the flagship modes; raw/ECB ciphers are never the default. |
| Password hashing | Argon2id with unique salts; PBKDF2/scrypt available for legacy interop. |
| Constant-time verification | All MACs, authentication tags, passwords, and signatures are compared in constant time. |
| Secret hygiene | `SecretBytes` / `SecretString` zeroize on drop; OS memory locking (`VirtualLock` on Windows, `mlock` on POSIX) prevents secrets from being paged to disk. |
| JWT safety | Verifiers enforce an explicit algorithm allow-list to prevent `alg:none` / algorithm-confusion attacks. |
| Key management | Ed25519 and X25519 preferred; RSA only in the SAFE forms; nonces are enforced unique per key. |
| Legacy algorithms | MD5, SHA-1, DES, and ECB exist **only** for interop and are marked legacy; using them for new security work is a misuse, not a library feature. |

**Deprecation policy:** cryptography is unforgiving. If a recommended algorithm becomes weak, it is deprecated and then removed; code using it breaks loudly rather than silently degrading.

### 5. Supply chain — ADL package integrity

- `adesh.lock` pins resolved dependency versions; changes are machine-checked.
- Package contents are verified with **SHA-256** checksums before loading from `adl_modules/`.
- Registry packages are compared against their recorded digests at install time.
- A bypass of these integrity checks (e.g. a lockfile/checksum bypass that lets a tampered package load) is a **critical supply-chain vulnerability**.

### 6. Execution backends & sandboxing

- Interpreter, VM, JIT (Cranelift), and AOT (LLVM via supplied toolchain) all execute the *same* safety-checked semantics; code-generation bugs that reintroduce memory corruption are vulnerabilities.
- **WASM backend:** execution runs inside `wasmtime`, a sandboxed WebAssembly runtime. Escape from the WASM sandbox to the host is a critical vulnerability.
- `Process.exec` and `Process` module APIs intentionally execute host commands — they are an application-level capability, not a sandbox boundary. Sanitize arguments at the application layer.

### 7. Language Server (ALS) & tooling

- `als` parses attacker-controlled documents (IDE scenario). Issues in `als` are in scope: parser panics, pathological-input resource exhaustion, or crashes triggered by malformed source are (at minimum) availability issues.
- The ALS never executes project code on hover/completion; any path that does is a vulnerability.

### 8. ATP protocol

- The Adesh Transport Protocol has its own security specification (`docs/ATP.md`, `docs/ATP_SECURITY.md` in the website docs tree). Report authentication, confidentiality, or integrity failures of ATP against its spec as protocol vulnerabilities.

---

## Known Limitations / Out of Scope

These are deliberately **not** covered by the security guarantee and are not vulnerabilities:

| Item | Why |
| :--- | :--- |
| Application logic flaws (auth, authorization, business logic) | The toolchain cannot make an application's policy correct. |
| Code run with `unsafe`, `extern`/FFI, raw pointers, `alloc`/`free` | By design, these delegate safety to the programmer. |
| Builds compiled with `allow_internal_network` | SSRF protection is explicitly opt-out via this flag. |
| Misuse of deprecated/legacy crypto (MD5, SHA-1, ECB) | Documented as legacy; using them for security-critical work is misuse. |
| Running untrusted programs outside the WASM sandbox | A bare process has host privileges by definition. |
| Secrets stored in source code, environment leakage, misconfigured deployments | Operational issues; report as documentation gaps if guidance is missing. |
| Upstream dependency vulnerabilities | Tracked via upstream advisories and dependency updates (see [Scope](#scope)). |

If you are unsure whether something is in scope, **report it anyway** — see the next section.

---

## Reporting a Vulnerability

### Preferred channel (private, encrypted)

**Use GitHub's Security Advisories feature — do NOT open a public issue for a security bug.**

1. Go to the repository → **Security** tab → **Report a vulnerability**:

   ```
   https://github.com/ajaytainwala-dev/mylang/security/advisories/new
   ```

2. You must have a GitHub account. Your report is **private** until a fix is released.
3. If the issue is extremely sensitive or you cannot use GitHub, email the maintainers:

   ```
   <SECURITY_EMAIL_PLACEHOLDER — maintainers: fill in a private address here>
   ```

   Encrypted reports are strongly preferred if the maintainers publish a PGP key:

   ```
   <PGP_KEY_PLACEHOLDER — maintainers: optional, link or fingerprint>
   ```

### What NOT to do

- ❌ Do **not** open a public GitHub issue, PR, or discussion describing the vulnerability.
- ❌ Do **not** post exploit code publicly before a fix ships.
- ❌ Do **not** attempt to exploit production systems, other users' machines, or third-party infrastructure to validate a bug (legal boundaries apply). Writing a proof-of-concept against your own build is fine.

### How to write a good report

The fastest way to get a fix is a complete, minimal report:

1. **Summary** — one or two sentences on the bug and its class (memory corruption, SSRF, type confusion, sandbox escape, DoS, ...).
2. **Affected components & versions** — which module and which release(s) you tested (`adesh --version`).
3. **Steps to reproduce** — ideally a self-contained `.adesh` file or minimal crate/build invocation. Include the exact command line and any feature flags used.
4. **Impact** — what an attacker can achieve (RCE, data exfiltration, crash/DoS, integrity bypass) and under what conditions (local vs remote, privileged vs unprivileged, needs untrusted code execution or not).
5. **Proof of concept** — as an attachment or a private gist link, never in a public place.
6. **Suggested fix / references** (optional) — if you already know the root cause, include it.

### Severity guidance (how we triage)

| Severity | Typical class | Example |
| :--- | :--- | :--- |
| **Critical** | Memory corruption, sandbox escape, RCE via parsing/serialization | Arbitrary code execution from a crafted `.adesh` file; WASM sandbox escape; network code exposing internal hosts WITHOUT `allow_internal_network`. |
| **High** | Auth/trust bypass, broken crypto property, supply-chain bypass | JWT algorithm confusion; AEAD nonce reuse; package checksum bypass. |
| **Medium** | DoS, unbounded resource use, info disclosure | Parser stack overflow / quadratic blowup on crafted input; TLS error verbose disclosure. |
| **Low** | Minor info leaks, hardening gaps, misuse resistance | Error messages disclosing file paths; missing or weak validation on an internal API. |

We use the CVSS model (where the advisory platform requires it) but the table above drives decisions.

---

## Vulnerability Handling Workflow

```
Report received (private)
   │
   ▼
[1] Triage .......... ≤ 48 hours   (acknowledge, assign severity, confirm in scope)
   │
   ▼
[2] Reproduce ....... ≤ 5 business days  (reproduce on latest supported release)
   │
   ▼
[3] Fix & test ...... with the release cadence for the severity
   │
   ▼
[4] Release ......... patch/minor release with the fix
   │
   ▼
[5] Disclose ........ advisory published (CVE if assigned) + credit
```

| Step | SLA | Notes |
| :--- | :--- | :--- |
| **Acknowledgment** | Within **48 hours** of receipt | We confirm receipt and give you a tracking reference. |
| **Triage decision** | Within **5 business days** | Accepted (confirmed, fix planned), needs more info, or not-in-scope (with explanation). |
| **Fix target** | Critical/High: as soon as possible, typically **within 14–30 days** of confirmation. Medium/Low: with the next normal release (typically a few weeks). | Times are targets, not guarantees; complex fixes may take longer and we will tell you. |
| **Status updates** | At least every **7 days** while the fix is open | You will not be left in the dark; if we cannot keep a date we say so. |
| **Disclosure** | After a fix is released and users have had a chance to upgrade | See [Coordinated Disclosure](#coordinated-disclosure). |

### What happens when a report is accepted

1. We confirm the vulnerability and add it to our private fix queue.
2. A fix is developed with a regression test; the commit message honors the reporter.
3. A patch release or patch-in-next-release is prepared and published.
4. The GitHub Security Advisory is opened/updated **after** the fix ships, with the affected versions and fixed version, and a CVE is requested when warranted.
5. The reporter is offered credit (see [Recognition](#recognition)).

### What happens when a report is declined or not-in-scope

- We reply with a clear technical explanation of why (e.g. out-of-scope misuse, duplicate, not reproducible after verification).
- Not-in-scope does not mean "ignored": documentation gaps discovered this way get fixed in the docs.

---

## Coordinated Disclosure

- **Default timeline:** reporters are asked to give **90 days** between confirmation and public disclosure. If a fix ships earlier, disclosure can happen sooner; if we need more time, we will ask.
- **Early disclosure** may be granted for: an actively exploited vulnerability, a fix already public, or where delaying disclosure further endangers users.
- **Simultaneous disclosure:** we coordinate with other vendors when the same upstream dependency is affected.
- **What we ask of reporters:** do not publish technical details or exploit code until the fixed release is available and the advisory is public. You may publish analysis once the advisory is out — and we will link yours from ours if you want.

---

## Security Hardening Guide for Users

1. **Always run the latest release.** Only the latest `0.3.x` receives security fixes.
2. **Do not build with `allow_internal_network` in production.** If you must, put a network-egress policy (deny-list / allow-list) in front of the process.
3. **Run untrusted code in the WASM backend or an OS sandbox**, never in a bare `adesh run` process with host privileges.
4. **Cryptography hygiene:**
   - Hash passwords with `crypto.passwordHash` (Argon2id) and unique salts.
   - Use AEAD (`crypto.seal`) rather than raw ciphers.
   - Never reuse nonces/IVs for a given key.
   - Reject MD5/SHA-1/DES/ECB for new security work.
   - Prefer `SecretBytes`/`SecretString` for key material.
5. **Validate anything that becomes a URL** before passing it to `HTTP`; SSRF protection is a backstop, not a substitute for input validation.
6. **Verify `adesh.lock` in version control** and review lockfile changes like you would review a diff of dependencies.
7. **Audit every `unsafe` block, `extern` declaration, and `alloc`/`free` pair** in code that receives untrusted input.
8. **Report early, report often** — a "maybe" is a valid report.

---

## Security FAQ

**Q: I found a bug in safe AdeshLang code that crashes the interpreter. Is that a vulnerability?**
A: Possibly — a crash reachable from safe code is at minimum a DoS; if it looks like memory corruption, report it as Critical.

**Q: Is AdeshLang a memory-safe language?**
A: Yes for safe code: compile-time ownership/borrowing with deterministic ARC prevent the classic C/C++ memory-safety bug classes. `unsafe` and FFI are excluded by design.

**Q: How do I verify the checksum of a package from the ADL registry?**
A: `adesh.lock` and the install path record SHA-256 digests which are verified before `adl_modules/` loading. If you suspect a mismatch, report it to us.

**Q: Can I use `allow_internal_network` just for development?**
A: Yes — it exists for local development where an app legitimately talks to local services. Ensure release builds do not enable it.

**Q: What do you do with upstream (crate-level) vulnerabilities?**
A: We track upstream advisories for our dependency tree and bump dependencies in a release; the fix note will reference the upstream CVE.

---

## Recognition

We list security researchers in the release notes and advisory **only with their consent**. If you report a vulnerability and want to be credited, tell us the name/handle to use and whether we may link a personal site. If you prefer anonymity, we will publish the finding without attribution.

---

## Contact

| Purpose | Channel |
| :--- | :--- |
| Private vulnerability reports | GitHub Security Advisories: `https://github.com/ajaytainwala-dev/mylang/security/advisories/new` |
| Encrypted/email fallback | `<SECURITY_EMAIL_PLACEHOLDER — maintainers: fill in>` |
| PGP key | `<PGP_KEY_PLACEHOLDER — maintainers: optional link or fingerprint>` |
| Public security questions (non-sensitive) | GitHub Discussions on the repository |
| Everything security-related, including "is this in scope?" | Same private channels — prefer asking privately |

**Maintainers:** before tagging your first public release, replace the two `<..._PLACEHOLDER>` values above with a real, monitored security contact and optionally a PGP key fingerprint. Keep this document linked from the repository root and the Readme's security section.

---

*This policy is a living document. Changes are announced in release notes; significant policy changes will be flagged in the changelog (`docs/CHANGELOG_AND_META.md`).*
