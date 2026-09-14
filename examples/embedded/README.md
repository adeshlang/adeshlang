# Embedded-Safe Examples

- `embedded_safe.adesh`
  - Arena-only workflow for embedded targets (heap + ARC disabled).
  - Run: `adesh run examples/embedded/embedded_safe.adesh --embedded`
  - Expected output:
    ```
    frame processed with arena-only allocations
    ```

- `arc_forbidden.adesh`
  - Shows ARC being rejected under embedded builds.
  - Run: `adesh run examples/embedded/arc_forbidden.adesh --embedded`
  - Expected: compile error stating ARC/`share` is not allowed when `--embedded` is enabled.
