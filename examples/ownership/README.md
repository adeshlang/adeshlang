# Ownership Examples

- `move_error.adesh`
  - Shows move semantics and use-after-move rejection.
  - Run: `adesh run examples/ownership/move_error.adesh`
  - Expected: compile error about using `r` after it was moved to `moved`.

- `debug_vs_release.adesh`
  - Demonstrates deterministic drops and debug-only safety checks around manual frees.
  - Run (debug): `adesh --profile debug run examples/ownership/debug_vs_release.adesh`
  - Run (release): `adesh --profile release run examples/ownership/debug_vs_release.adesh`
  - Expected (debug): program aborts with double-free detection after the second `free`.
  - Expected (release): no metadata; behavior after double free is undefined (intentional to show why debug tooling matters).
