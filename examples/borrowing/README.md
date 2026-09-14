# Borrowing Examples

- `borrowing_rules.adesh`
  - Multiple immutable borrows with zero runtime cost in release.
  - Run: `adesh run examples/borrowing/borrowing_rules.adesh`
  - Expected output:
    ```
    6
    6
    ```

- `invalid_mutable_borrow.adesh`
  - Shows compile-time rejection of mutable + immutable borrows.
  - Run: `adesh run examples/borrowing/invalid_mutable_borrow.adesh`
  - Expected: compile error indicating `numbers` cannot be mutably borrowed while immutably borrowed.
