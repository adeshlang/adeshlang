# Incremental Linking & Caching (`incremental`)

The incremental linking subsystem provides fast relinking cycles for iterative development.

---

## 1. Hash-Based Invalidation Model

The linker computes persistent content hashes for:
1. **Target Triple & Linker Configuration**: Changes to flags, targets, or linker options invalidate the entire cache.
2. **Object File Hashes**: SHA256 hashes of input object contents and archive members.
3. **Symbol Table Hashes**: Signatures of exported and referenced global symbols.
4. **Relocation Hashes**: Dependency records between sections.

---

## 2. Link Cache Directory

By default, link state is stored under `.adesh/link-cache/` (configurable via `--cache-dir <dir>` or disabled via `--no-cache`).

### Cache Contents
- `manifest.json`: Link inputs, target metadata, configuration hash, and timestamp.
- `symbols.cache`: Serialized symbol resolution database.
- `layout.cache`: Virtual address layout assignments.
