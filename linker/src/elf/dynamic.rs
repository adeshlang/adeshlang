//! ELF dynamic-linking metadata: dynamic relocations, import/export bookkeeping,
//! GNU hash table construction.
//!
//! The layout engine records dynamic relocations while it applies static
//! relocations to the output image; the ELF writer consumes them to emit
//! `.rela.dyn` / `.rela.plt` and the matching `DT_*` entries.

use crate::elf::header::gnu_hash;
use std::collections::HashMap;

/// A dynamic relocation to be emitted into `.rela.dyn` / `.rela.plt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElfDynamicReloc {
    /// Virtual address of the storage unit to relocate (`r_offset`),
    /// expressed in the link-time address space (the loader adds the load
    /// bias for `ET_DYN` images).
    pub offset: u64,
    pub kind: ElfDynamicRelocKind,
    /// Dynamic symbol name the relocation binds against (`None` for
    /// base-relative relocations such as `R_X86_64_RELATIVE`).
    pub symbol: Option<String>,
    /// `r_addend`.
    pub addend: i64,
}

/// Dynamic relocation kinds emitted by Adesh links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfDynamicRelocKind {
    /// `R_X86_64_RELATIVE` / `R_AARCH64_RELATIVE`: `*offset = load_bias + addend`.
    Relative,
    /// `R_X86_64_JUMP_SLOT` / `R_AARCH64_JUMP_SLOT`: PLT GOT slot,
    /// `*offset = resolve(symbol)`. Bound eagerly (`DT_BIND_NOW`).
    JumpSlot,
}

impl ElfDynamicRelocKind {
    /// ABI relocation number for the given architecture, or `None` when the
    /// architecture has no dynamic-relocation support in this linker (the
    /// link must then fail loudly instead of emitting an invalid number).
    pub fn abi_number(&self, arch: crate::target::Arch) -> Option<u32> {
        match arch {
            crate::target::Arch::X86_64 => match self {
                ElfDynamicRelocKind::Relative => Some(crate::elf::header::R_X86_64_RELATIVE),
                ElfDynamicRelocKind::JumpSlot => Some(crate::elf::header::R_X86_64_JUMP_SLOT),
            },
            crate::target::Arch::AArch64 => match self {
                ElfDynamicRelocKind::Relative => Some(crate::elf::header::R_AARCH64_RELATIVE),
                ElfDynamicRelocKind::JumpSlot => Some(crate::elf::header::R_AARCH64_JUMP_SLOT),
            },
            _ => None,
        }
    }
}

/// Everything the ELF writer needs to produce a dynamically linked image.
///
/// Constructed by the link driver from the layout results; consumed by
/// `ElfWriter::encode_elf`.
#[derive(Debug, Clone, Default)]
pub struct ElfLinkInfo {
    /// Position-independent executable (`ET_DYN` + `DF_1_PIE` +
    /// `R_X86_64_RELATIVE` relocations).
    pub pie: bool,
    /// Dynamic import names (undefined `.dynsym` entries), deterministically
    /// ordered. Index `i` in this list occupies `.dynsym` slot `i + 1`.
    pub imports: Vec<String>,
    /// Virtual address of each import's GOT slot (from the layout engine).
    /// `None` entries mean no GOT slot (data imports are rejected earlier).
    pub got_slots: HashMap<String, u64>,
    /// Dynamic relocations (both `.rela.dyn` and `.rela.plt` entries).
    pub dynamic_relocs: Vec<ElfDynamicReloc>,
    /// `PT_INTERP` path. `None` for static executables and shared libraries.
    pub interpreter: Option<String>,
    /// `DT_NEEDED` sonames.
    pub needed_libs: Vec<String>,
    /// `DT_SONAME` for shared libraries.
    pub soname: Option<String>,
}

impl ElfLinkInfo {
    /// Whether the emitted ELF must be `ET_DYN` (PIE executable or shared
    /// library).
    pub fn is_et_dyn(&self, is_shared: bool) -> bool {
        is_shared || self.pie
    }

    /// Whether any dynamic-linking metadata must be emitted at all.
    pub fn is_dynamic(&self) -> bool {
        self.pie
            || !self.imports.is_empty()
            || !self.dynamic_relocs.is_empty()
            || !self.needed_libs.is_empty()
            || self.soname.is_some()
            || self.interpreter.is_some()
    }
}

/// Build a GNU hash table (`.gnu.hash`) over `names`.
///
/// Each `names[i]` will occupy the `.dynsym` slot given by
/// `table.order[i] + first_hashed`. The chain array stores masked GNU hash
/// values with a low terminating bit (the format glibc/musl implement):
/// a bucket walk starts at `buckets[b]` and proceeds contiguously through
/// the chain array until an entry with its low bit set.
///
/// `first_hashed` must be >= 1: `.dynsym` slot 0 is the null symbol and is
/// never hashed.
pub fn build_gnu_hash(names: &[&str], first_hashed: u32) -> GnuHashTable {
    let n = names.len();
    // Bucket count: a small power of two, proportional to symbol count.
    let mut nbuckets: u32 = 1;
    while nbuckets < 8 && (nbuckets as usize) < n / 4 {
        nbuckets *= 2;
    }
    let nbuckets = nbuckets.max(1);

    // Stable order: bucket ascending, then original index — symbols of one
    // bucket must be contiguous for the contiguous chain walk.
    let mut order: Vec<usize> = (0..n).collect();
    let bucket_of = |idx: usize| gnu_hash(names[idx].as_bytes()) % nbuckets;
    order.sort_by_key(|&idx| (bucket_of(idx), idx));

    let mut buckets = vec![0u32; nbuckets as usize];
    // chain[i] belongs to hashed slot i (relative to the hashed region).
    let mut chains = vec![0u32; n];

    for chain_pos in 0..n {
        let name_idx = order[chain_pos];
        let h = gnu_hash(names[name_idx].as_bytes());
        let b = bucket_of(name_idx) as usize;
        if buckets[b] == 0 {
            buckets[b] = first_hashed + chain_pos as u32;
        }
        // Low bit set terminates the bucket walk: last symbol of its bucket.
        let is_last = chain_pos + 1 == n || bucket_of(order[chain_pos + 1]) as usize != b;
        chains[chain_pos] = (h & !1) | is_last as u32;
    }

    // Bloom filter: 64-bit words; bits `hash % 64` and `(hash >> shift) % 64`
    // of words `(hash / 64) % bloom_size` (and shifted variant).
    const BLOOM_SHIFT: u32 = 6; // 6 is conventional for GNU hash
    let bloom_size: u32 = (nbuckets + 63) / 64;
    let mut bloom = vec![0u64; bloom_size as usize];
    for &name_idx in &order {
        let h = gnu_hash(names[name_idx].as_bytes());
        bloom[(h / 64) as usize % bloom_size as usize] |= 1u64 << (h % 64);
        bloom[((h >> BLOOM_SHIFT) / 64) as usize % bloom_size as usize] |=
            1u64 << ((h >> BLOOM_SHIFT) % 64);
    }

    GnuHashTable {
        nbuckets,
        bloom,
        bloom_shift: BLOOM_SHIFT,
        buckets,
        chains,
        order,
    }
}

/// A built GNU hash table plus the `.dynsym` permutation it requires.
#[derive(Debug, Clone)]
pub struct GnuHashTable {
    pub nbuckets: u32,
    pub bloom: Vec<u64>,
    pub bloom_shift: u32,
    /// Absolute `.dynsym` slot of each bucket's first symbol.
    pub buckets: Vec<u32>,
    /// Masked hash values with terminating low bit, hashed-region-relative.
    pub chains: Vec<u32>,
    /// `order[i]` = index into the input name list that hashed slot `i`
    /// (absolute slot `i + first_hashed`) must contain.
    pub order: Vec<usize>,
}

impl GnuHashTable {
    /// Encode the table for the file, given `first_hashed` (the first
    /// `.dynsym` slot covered by this table; must be >= 1 because slot 0 is
    /// the null symbol).
    pub fn encode(&self, first_hashed: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.nbuckets.to_le_bytes());
        out.extend_from_slice(&first_hashed.to_le_bytes());
        out.extend_from_slice(&(self.bloom.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bloom_shift.to_le_bytes());
        for w in &self.bloom {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for b in &self.buckets {
            out.extend_from_slice(&b.to_le_bytes());
        }
        for c in &self.chains {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::header::gnu_hash;

    /// glibc-compatible `.gnu.hash` lookup over a simulated dynsym: walk the
    /// chain array contiguously from the bucket start, comparing masked hash
    /// values, until a chain entry with its terminating low bit set.
    fn gnu_lookup<'a>(
        table: &GnuHashTable,
        names: &'a [&'a str],
        slot_of_name: &[usize],
        first_hashed: u32,
        query: &str,
    ) -> Option<&'a str> {
        let h = gnu_hash(query.as_bytes());
        let bucket = (h % table.nbuckets) as usize;
        let start = table.buckets[bucket];
        if start == 0 || start < first_hashed {
            return None;
        }
        let bloom_ok = {
            let w = table.bloom[(h / 64) as usize % table.bloom.len()];
            (w >> (h % 64)) & 1 == 1
                && {
                    let w2 =
                        table.bloom[((h >> table.bloom_shift) / 64) as usize % table.bloom.len()];
                    (w2 >> ((h >> table.bloom_shift) % 64)) & 1 == 1
                }
        };
        if !bloom_ok {
            return None;
        }
        let masked = h & !1;
        let mut rel = (start - first_hashed) as usize;
        loop {
            assert!(rel < table.chains.len(), "chain walk ran off the table");
            let entry = table.chains[rel];
            if (entry & !1) == masked {
                // Slot start maps back to a name; verify by string as the
                // loader's second-stage comparison does.
                let name_idx = table.order[rel];
                if names[name_idx] == query {
                    let _ = slot_of_name;
                    return Some(names[name_idx]);
                }
            }
            if entry & 1 == 1 {
                return None;
            }
            rel += 1;
        }
    }

    #[test]
    fn gnu_hash_table_resolves_every_name() {
        const FIRST_HASHED: u32 = 1;
        let names = vec!["puts", "printf", "malloc", "free", "memcpy", "strlen", "exit"];
        let table = build_gnu_hash(&names, FIRST_HASHED);

        for name in &names {
            assert_eq!(
                gnu_lookup(&table, &names, &[], FIRST_HASHED, name),
                Some(*name),
                "gnu.hash lookup failed for `{name}`"
            );
        }
        assert_eq!(
            gnu_lookup(&table, &names, &[], FIRST_HASHED, "absent_symbol"),
            None
        );
    }

    #[test]
    fn gnu_hash_bucket_walks_terminate() {
        let names: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];
        let table = build_gnu_hash(&names, 1);
        // Every non-zero bucket must start inside the hashed region and its
        // walk must terminate on a low-bit entry within bounds.
        for &start in &table.buckets {
            if start == 0 {
                continue;
            }
            assert!(start >= 1 && (start - 1) < table.chains.len() as u32);
            let mut rel = (start - 1) as usize;
            let mut hops = 0;
            loop {
                assert!(rel < table.chains.len(), "chain ran off the table");
                let entry = table.chains[rel];
                hops += 1;
                if entry & 1 == 1 {
                    break;
                }
                rel += 1;
                assert!(hops <= names.len() + 1, "chain did not terminate");
            }
        }
    }

    #[test]
    fn bucket_walks_cover_every_hashed_slot_exactly_once() {
        let names: Vec<&str> = (0..24)
            .map(|i| match i % 5 {
                0 => "alpha",
                1 => "beta",
                2 => "gamma",
                3 => "delta",
                _ => "epsilon",
            })
            .chain(std::iter::once("zeta"))
            .collect();
        let table = build_gnu_hash(&names, 1);
        // Collect all slots visited by all bucket walks: they must cover the
        // hashed region exactly once (no slot double-reachable, none orphaned).
        let mut visited = vec![false; names.len()];
        for &start in &table.buckets {
            if start == 0 {
                continue;
            }
            let mut rel = (start - 1) as usize;
            loop {
                assert!(!visited[rel], "hashed slot {rel} reachable twice");
                visited[rel] = true;
                if table.chains[rel] & 1 == 1 {
                    break;
                }
                rel += 1;
            }
        }
        assert!(visited.iter().all(|&v| v), "some hashed slot is unreachable");
    }

    #[test]
    fn dyn_reloc_abi_numbers_are_correct() {
        assert_eq!(
            ElfDynamicRelocKind::Relative.abi_number(crate::target::Arch::X86_64),
            Some(8)
        );
        assert_eq!(
            ElfDynamicRelocKind::JumpSlot.abi_number(crate::target::Arch::X86_64),
            Some(7)
        );
        assert_eq!(
            ElfDynamicRelocKind::Relative.abi_number(crate::target::Arch::Riscv64),
            None,
            "unsupported targets must fail loudly, not emit a bogus number"
        );
    }
}
