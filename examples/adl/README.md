# ADL Ecosystem Examples

This directory demonstrates the additive ADL ecosystem features and the project-local package flow:

- `adesh.adl` manifest format
- `adesh.lock.adl` generated dependency state
- Local project package layout under `.adl/`
- App and library source templates
- Manifest-driven dependency resolution and graph inspection
- Sensitive values are removed from `adesh.adl` and stored in `adesh.lock.adl`

For the full workflow guide, see [ADL_ECOSYSTEM_USAGE_GUIDE.md](../../ADL_ECOSYSTEM_USAGE_GUIDE.md).

## Example Commands

```bash
adl init
adl install
adl build
adl add core ^1.0.0
adl tree
adl why core
```

## Files

- `adesh.adl` - project manifest
- `adesh.lock.adl` - deterministic lockfile example
- `main.adesh` - app entrypoint example
- `lib.adesh` - library example
- `README.md` - local example guide
