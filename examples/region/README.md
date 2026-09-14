# Region / Arena Examples

- `region_scope.adesh`
  - Shows arena-backed region with bulk free.
  - Run: `adesh run examples/region/region_scope.adesh`
  - Expected output:
    ```
    region objects live Obj(...) [1, 2, 3]
    region freed; header and payload dropped
    ```

- `region_escape_error.adesh`
  - Demonstrates compile-time error when a reference escapes a region.
  - Run: `adesh run examples/region/region_escape_error.adesh`
  - Expected: compile error indicating the reference to `inner` cannot escape the `Temp` region.
