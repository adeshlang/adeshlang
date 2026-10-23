//! dyld dynamic-linking machinery for Mach-O 64: compressed rebasing,
//! binding, lazy binding, and the export trie (`LC_DYLD_INFO_ONLY`).
//!
//! These encoders implement the opaque byte formats dyld actually parses at
//! load time (see `mach-o/loader.h` and dyld's `ImageLoaderMachOCompressed`).
//! Every encoder here is paired with a reference *decoder* in the unit tests
//! below so the byte streams are validated by re-reading them, not by
//! trusting the writer.
//!
//! x86_64 lazy-binding flow implemented by the writer on top of these
//! encoders:
//!
//! ```text
//! call site ──call──▶ proxy stub ──jmp *la_slot──▶ __stub_helper entry_i
//!                                                    │ push lazy_offset_i
//!                                                    ▼
//!                                              push image_slide_marker
//!                                              jmp *dyld_stub_binder@GOT
//! ```
//! `dyld_stub_binder` reads the pushed lazy offset, resolves the symbol,
//! patches `__la_symbol_ptr[i]`, and jumps to the real function so the
//! original call proceeds transparently.

// ─── dyld rebase opcodes (mach-o/loader.h) ─────────────────────────────────

pub const REBASE_OPCODE_DONE: u8 = 0x00;
pub const REBASE_OPCODE_SET_TYPE_IMM: u8 = 0x10;
pub const REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB: u8 = 0x30;
pub const REBASE_OPCODE_DO_REBASE_IMM_TIMES: u8 = 0x40;
pub const REBASE_OPCODE_ADD_ADDR_ULEB: u8 = 0x50;
pub const REBASE_OPCODE_ADD_ADDR_IMM_SCALED: u8 = 0x60;
pub const REBASE_OPCODE_DO_REBASE_ULEB_TIMES: u8 = 0x80;
pub const REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB: u8 = 0x90;
pub const REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB: u8 = 0xC0;

/// Fixup applied to a location. `Pointer` rewrites a full 64-bit word;
/// `TextAbsolute32` rewrites a 32-bit immediate embedded in TEXT (the stub
/// helper's `push $helper_addr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseKind {
    Pointer,
    TextAbsolute32,
}

impl RebaseKind {
    pub fn type_imm(self) -> u8 {
        match self {
            RebaseKind::Pointer => REBASE_TYPE_POINTER,
            RebaseKind::TextAbsolute32 => REBASE_TYPE_TEXT_ABSOLUTE32,
        }
    }
}

pub const REBASE_TYPE_POINTER: u8 = 1;
pub const REBASE_TYPE_TEXT_ABSOLUTE32: u8 = 2;

// ─── dyld bind opcodes ─────────────────────────────────────────────────────

pub const BIND_OPCODE_DONE: u8 = 0x00;
pub const BIND_OPCODE_SET_DYLIB_ORDINAL_IMM: u8 = 0x10;
pub const BIND_OPCODE_SET_DYLIB_ORDINAL_ULEB: u8 = 0x20;
pub const BIND_OPCODE_SET_DYLIB_SPECIAL_IMM: u8 = 0x30;
pub const BIND_OPCODE_SET_SYMBOL_TRAILING_FLAGS_IMM: u8 = 0x40;
pub const BIND_OPCODE_SET_TYPE_IMM: u8 = 0x50;
pub const BIND_OPCODE_SET_ADDEND_SLEB: u8 = 0x60;
pub const BIND_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB: u8 = 0x70;
pub const BIND_OPCODE_ADD_ADDR_ULEB: u8 = 0x80;
pub const BIND_OPCODE_DO_BIND: u8 = 0x90;
pub const BIND_OPCODE_DO_BIND_ADD_ADDR_IMM_SCALED: u8 = 0xA0;
pub const BIND_OPCODE_DO_BIND_ADD_ADDR_ULEB: u8 = 0xB0;
pub const BIND_OPCODE_DO_BIND_ULEB_TIMES_SKIPPING_ULEB: u8 = 0xC0;

pub const BIND_TYPE_POINTER: u8 = 1;

pub const BIND_SYMBOL_FLAGS_WEAK_IMPORT: u8 = 0x1;

/// `dyld_stub_binder` is exported by libSystem; ordinal 1 is the first
/// `LC_LOAD_DYLIB` (libSystem is always emitted first by the writer).
pub const DYLD_STUB_BINDER: &str = "dyld_stub_binder";
pub const LIBSYSTEM_ORDINAL: u8 = 1;

/// dyld's thread-local-variable bootstrap accessor. Each
/// `S_THREAD_LOCAL_VARIABLES` descriptor's `thunk` field is bound to it; the
/// address only exists once the image is loaded, so it is a data bind and not
/// a call — it owns no `__la_symbol_ptr` slot and no lazy stub.
pub const TLV_BOOTSTRAP: &str = "__tlv_bootstrap";

// ─── export trie flags ─────────────────────────────────────────────────────

pub const EXPORT_SYMBOL_FLAGS_KIND_MASK: u64 = 0x03;
pub const EXPORT_SYMBOL_FLAGS_KIND_REGULAR: u64 = 0x00;
pub const EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE: u64 = 0x02;
pub const EXPORT_SYMBOL_FLAGS_WEAK_DEFINITION: u64 = 0x04;

// ─── public link-info model ────────────────────────────────────────────────

/// One function imported from a dylib.
#[derive(Debug, Clone)]
pub struct MachOImport {
    /// Name as referenced by the input objects (e.g. `puts`).
    pub raw_name: String,
    /// Name dyld binds against: the Mach-O symbol with leading underscore
    /// (e.g. `_puts`).
    pub bind_name: String,
    /// Virtual address of this import's `__la_symbol_ptr` slot.
    pub la_slot_va: u64,
    /// Data-object imports are rejected loudly by the layout stage, so this
    /// is always `false` today; kept for future GOT-based data binding.
    pub is_data: bool,
}

/// One exported symbol of a shared library (export trie entry).
#[derive(Debug, Clone)]
pub struct MachOExport {
    pub name: String,
    pub address: u64,
}

/// A non-lazy data bind the layout stage produced but the writer cannot
/// synthesize itself: `(virtual address of the pointer slot, bind symbol,
/// addend)`. Today this is exactly the TLV descriptor thunk fields that point
/// at `__tlv_bootstrap`; the slot lives in `__DATA`, so the writer maps the VA
/// to `(segment_index, offset)` when it emits the bind stream.
#[derive(Debug, Clone)]
pub struct MachODataBind {
    pub va: u64,
    pub symbol: String,
    pub addend: i64,
}

/// Everything the Mach-O writer needs to emit `LC_DYLD_INFO_ONLY` and the
/// indirect symbol table. Assembled by the linker from layout results.
#[derive(Debug, Clone, Default)]
pub struct MachODynamicInfo {
    /// Function imports, sorted by `raw_name`; index i owns
    /// `__la_symbol_ptr` slot i.
    pub imports: Vec<MachOImport>,
    /// Virtual addresses of 64-bit pointer words baked at link time that the
    /// loader must rebase when the image slides (PIE). Sorted by the writer.
    pub rebases: Vec<u64>,
    /// Non-lazy data binds recorded by the layout stage (TLV descriptor
    /// thunks). Merged with the writer's own `dyld_stub_binder` GOT bind when
    /// the bind stream is encoded.
    pub binds: Vec<MachODataBind>,
    /// Exports of a shared library (empty for executables).
    pub exports: Vec<MachOExport>,
}

/// A single rebase target after the writer has mapped a VA into its segment.
#[derive(Debug, Clone, Copy)]
pub struct RebaseTarget {
    pub segment_index: u8,
    /// Byte offset within the segment (what dyld's rebase stream encodes).
    pub offset: u64,
    pub kind: RebaseKind,
}

/// A single non-lazy bind target.
#[derive(Debug, Clone)]
pub struct BindTarget {
    pub segment_index: u8,
    pub offset: u64,
    pub bind_type: u8,
    pub ordinal: u8,
    pub symbol: String,
    pub addend: i64,
}

/// One lazy-bind entry (one per `__la_symbol_ptr` slot).
#[derive(Debug, Clone)]
pub struct LazyBindTarget {
    pub segment_index: u8,
    pub offset: u64,
    pub ordinal: u8,
    pub symbol: String,
}

// ─── LEB128 ─────────────────────────────────────────────────────────────────

pub fn write_uleb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub fn write_sleb128(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let sign_set = (byte & 0x40) != 0;
        if (value == 0 && !sign_set) || (value == -1 && sign_set) {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub fn uleb128_len(value: u64) -> usize {
    let mut n = 1;
    let mut v = value >> 7;
    while v != 0 {
        n += 1;
        v >>= 7;
    }
    n
}

// ─── rebase stream ─────────────────────────────────────────────────────────

/// Encode a sorted `(segment_index, offset)` rebase stream.
///
/// Targets must be pre-sorted by `(segment_index, offset, kind-class)` so the
/// emitted stream is deterministic for identical inputs. The encoder batches
/// consecutive same-type runs (`DO_REBASE_IMM_TIMES`), emits
/// `ADD_ADDR_IMM_SCALED` for 8-byte-stride gaps, and falls back to a fresh
/// `SET_SEGMENT_AND_OFFSET` for larger jumps or type changes.
pub fn encode_rebase_opcodes(targets: &[RebaseTarget]) -> Vec<u8> {
    let mut out = Vec::new();
    if targets.is_empty() {
        out.push(REBASE_OPCODE_DONE);
        return out;
    }

    let mut current_seg: Option<u8> = None;
    let mut current_type: Option<u8> = None;
    let mut current_addr: u64 = 0;
    // Address of the next byte dyld would fix up if we told it to rebase
    // again without moving the cursor.
    let mut have_addr = false;
    let mut i = 0usize;

    while i < targets.len() {
        let target = &targets[i];

        if current_type != Some(target.kind.type_imm()) {
            out.push(REBASE_OPCODE_SET_TYPE_IMM | target.kind.type_imm());
            current_type = Some(target.kind.type_imm());
        }

        let fresh_segment = current_seg != Some(target.segment_index)
            || !have_addr
            || target.offset != current_addr;

        if fresh_segment {
            out.push(REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | target.segment_index);
            write_uleb128(&mut out, target.offset);
            current_seg = Some(target.segment_index);
            current_addr = target.offset;
            have_addr = true;
        }

        // Count the run: consecutive targets at stride 8 (pointer slots) with
        // the same fixup type and segment.
        let stride = match target.kind {
            // A 64-bit pointer fixup advances the cursor by 8.
            RebaseKind::Pointer => 8u64,
            // A 32-bit immediate fixup advances by 4.
            RebaseKind::TextAbsolute32 => 4u64,
        };
        let mut run = 0usize;
        while i + run < targets.len() {
            let next = &targets[i + run];
            if next.segment_index != target.segment_index
                || next.kind != target.kind
                || next.offset != target.offset + (run as u64) * stride
            {
                break;
            }
            run += 1;
        }

        let mut remaining = run;
        while remaining > 0 {
            let chunk = remaining.min(15);
            out.push(REBASE_OPCODE_DO_REBASE_IMM_TIMES | chunk as u8);
            remaining -= chunk;
        }

        current_addr += (run as u64) * stride;
        i += run;

        // Skip a uniform gap with the scaled immediate form when possible;
        // otherwise the next iteration opens a fresh segment/offset record.
        if i < targets.len() {
            let next = &targets[i];
            let gap = next.offset as i64 - current_addr as i64;
            if next.segment_index == current_seg.unwrap_or(0)
                && next.kind == targets[i - 1].kind
                && gap > 0
                && gap % (stride as i64) == 0
                && (gap / stride as i64) <= 15
                && gap / stride as i64 > 0
            {
                let scaled = (gap / stride as i64) as u8;
                out.push(REBASE_OPCODE_ADD_ADDR_IMM_SCALED | scaled);
                current_addr = next.offset;
            }
        }
    }

    out.push(REBASE_OPCODE_DONE);
    out
}

// ─── bind streams ──────────────────────────────────────────────────────────

/// Encode non-lazy bind entries (immediate binding at load time).
///
/// Each entry is fully self-contained (ordinal, symbol, type, address) —
/// dyld may start parsing anywhere in a bind stream it is told to, and this
/// property is what makes mid-stream entry points sound.
pub fn encode_bind_opcodes(targets: &[BindTarget]) -> Vec<u8> {
    let mut out = Vec::new();
    for target in targets {
        encode_bind_entry(
            &mut out,
            target.segment_index,
            target.offset,
            target.bind_type,
            target.ordinal,
            &target.symbol,
            target.addend,
        );
    }
    out.push(BIND_OPCODE_DONE);
    out
}

fn encode_bind_entry(
    out: &mut Vec<u8>,
    segment_index: u8,
    offset: u64,
    bind_type: u8,
    ordinal: u8,
    symbol: &str,
    addend: i64,
) {
    if ordinal > 0 && ordinal < 16 {
        out.push(BIND_OPCODE_SET_DYLIB_ORDINAL_IMM | ordinal);
    } else {
        out.push(BIND_OPCODE_SET_DYLIB_ORDINAL_ULEB);
        write_uleb128(out, ordinal as u64);
    }
    out.push(BIND_OPCODE_SET_TYPE_IMM | bind_type);
    out.push(BIND_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | segment_index);
    write_uleb128(out, offset);
    out.push(BIND_OPCODE_SET_SYMBOL_TRAILING_FLAGS_IMM);
    out.extend_from_slice(symbol.as_bytes());
    out.push(0);
    if addend != 0 {
        out.push(BIND_OPCODE_SET_ADDEND_SLEB);
        write_sleb128(out, addend);
    }
    out.push(BIND_OPCODE_DO_BIND);
}

/// Encode lazy-bind entries and return the byte offset of each entry within
/// the stream. The stub helper pushes entry i's offset so
/// `dyld_stub_binder` can resume parsing exactly there.
///
/// Entry i must correspond to `__la_symbol_ptr` slot i.
pub fn encode_lazy_bind_opcodes(targets: &[LazyBindTarget]) -> (Vec<u8>, Vec<u64>) {
    let mut out = Vec::new();
    let mut offsets = Vec::with_capacity(targets.len());
    for target in targets {
        offsets.push(out.len() as u64);
        // dyld's lazy-bind walker accepts only SET_SEGMENT_AND_OFFSET_ULEB,
        // the two dylib-ordinal opcodes, SET_SYMBOL_TRAILING_FLAGS_IMM,
        // DO_BIND and DONE. Anything else — including SET_TYPE_IMM, which
        // non-lazy streams use — is a hard "bad lazy bind opcode" error at
        // launch, because the fixup kind is implied by
        // S_LAZY_SYMBOL_POINTERS. Entry i binds __la_symbol_ptr slot i and
        // the stub helper jumps to offsets[i].
        out.push(BIND_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | target.segment_index);
        write_uleb128(&mut out, target.offset);
        if target.ordinal > 0 && target.ordinal < 16 {
            out.push(BIND_OPCODE_SET_DYLIB_ORDINAL_IMM | target.ordinal);
        } else {
            out.push(BIND_OPCODE_SET_DYLIB_ORDINAL_ULEB);
            write_uleb128(&mut out, target.ordinal as u64);
        }
        out.push(BIND_OPCODE_SET_SYMBOL_TRAILING_FLAGS_IMM);
        out.extend_from_slice(target.symbol.as_bytes());
        out.push(0);
        out.push(BIND_OPCODE_DO_BIND);
        out.push(BIND_OPCODE_DONE);
    }
    (out, offsets)
}

// ─── export trie ───────────────────────────────────────────────────────────

/// A node in the export trie while it is being built.
#[derive(Debug, Default)]
struct TrieNode {
    /// Terminal info (flags + address) when a symbol ends at this node.
    terminal: Option<(u64, u64)>,
    /// Child edges: (label bytes, child node index in a flat arena).
    children: Vec<(Vec<u8>, usize)>,
}

/// Build the dyld export trie for `exports`.
///
/// Byte format (what dyld's trie walker parses):
/// node := uleb terminal-size [terminal-size bytes: uleb flags, uleb address]
///         uleb child-count
///         per-child: byte label-len, label bytes, uleb absolute child-offset
///
/// The root node sits at offset 0 with no incoming edge. A node's serialized
/// size depends on the uleb width of its children's absolute offsets, and
/// those offsets depend on the sizes laid out before them, so offsets are
/// assigned by iterating to a fixed point: widths only ever grow (uleb widths
/// are monotone in the encoded value), which guarantees convergence.
pub fn build_export_trie(exports: &[MachOExport]) -> Vec<u8> {
    // Deterministic trie: sort by bind name; duplicate names keep the first
    // definition (stable sort + `dedup_by`).
    let mut sorted: Vec<&MachOExport> = exports.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    sorted.dedup_by(|a, b| a.name == b.name);

    let mut nodes: Vec<TrieNode> = Vec::new();
    if sorted.is_empty() {
        // Valid empty trie: root node with no terminal info and no children.
        // Both header fields are present (terminal-size uleb, child-count
        // uleb) — a lone 0x00 would make dyld's parser read past the blob.
        return vec![0, 0];
    }
    let entries: Vec<(&[u8], u64)> = sorted
        .iter()
        .map(|e| (e.name.as_bytes(), e.address))
        .collect();
    build_subtree(&mut nodes, &entries);

    // Fixed-point offset assignment. `widths[i]` is the assumed uleb width
    // of node i's offset (the field its parent writes); it grows until
    // stable. Each pass lays the tree out top-down with the current widths,
    // recomputing widths from the positions it actually assigned.
    let mut widths: Vec<usize> = vec![1; nodes.len()];
    let mut assigned: Vec<Option<u32>> = vec![None; nodes.len()];
    for _ in 0..64 {
        let mut changed = false;
        layout_subtree(&nodes, 0, 0, &mut widths, &mut assigned, &mut changed);
        if !changed {
            break;
        }
    }

    // Emit in the same DFS pre-order the assignment pass used, so every
    // child lands exactly at its assigned absolute offset.
    fn emit(nodes: &[TrieNode], idx: usize, assigned: &[Option<u32>], out: &mut Vec<u8>) {
        let node = &nodes[idx];
        let term_size = node_terminal_size(node);
        write_uleb128(out, term_size);
        if let Some((flags, address)) = node.terminal {
            write_uleb128(out, flags);
            write_uleb128(out, address);
        }
        write_uleb128(out, node.children.len() as u64);
        for (label, child) in &node.children {
            out.push(label.len() as u8);
            out.extend_from_slice(label);
            let child_offset = assigned[*child].expect("offset assigned in fixed point");
            write_uleb128(out, child_offset as u64);
        }
        for (_, child) in &node.children {
            emit(nodes, *child, assigned, out);
        }
    }

    let mut out = Vec::new();
    emit(&nodes, 0, &assigned, &mut out);
    out
}

/// Build the subtree covering `entries` (all sharing the prefix already
/// consumed) and return its node index. Children are grouped by their next
/// byte in ascending byte order, so identical inputs always produce
/// identical tries.
fn build_subtree(nodes: &mut Vec<TrieNode>, entries: &[(&[u8], u64)]) -> usize {
    let node_id = nodes.len();
    nodes.push(TrieNode::default());

    let mut terminal: Option<u64> = None;
    let mut rest: Vec<(&[u8], u64)> = Vec::new();
    for (suffix, address) in entries {
        if suffix.is_empty() {
            terminal = Some(*address);
        } else {
            rest.push((suffix, *address));
        }
    }
    if let Some(address) = terminal {
        nodes[node_id].terminal = Some((EXPORT_SYMBOL_FLAGS_KIND_REGULAR, address));
    }

    // Group remaining suffixes by their next byte. Inputs are sorted by name
    // (pre-condition from `build_export_trie`), so entries within a group
    // keep that order.
    let mut groups: std::collections::BTreeMap<u8, Vec<(&[u8], u64)>> = Default::default();
    for (suffix, address) in rest {
        groups.entry(suffix[0]).or_default().push((suffix, address));
    }
    for (_first_byte, group) in groups {
        // Shared edge label: the longest common prefix of the group (at
        // least the one byte that defines the group).
        let mut lcp = group[0].0.len();
        for (suffix, _) in &group {
            lcp = lcp.min(common_prefix_len(group[0].0, suffix));
        }
        let label = group[0].0[..lcp].to_vec();
        let child_entries: Vec<(&[u8], u64)> = group
            .iter()
            .map(|(suffix, address)| (&suffix[lcp..], *address))
            .collect();
        let child = build_subtree(nodes, &child_entries);
        nodes[node_id].children.push((label, child));
    }
    node_id
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn node_terminal_size(node: &TrieNode) -> u64 {
    match node.terminal {
        Some((flags, address)) => (uleb128_len(flags) + uleb128_len(address)) as u64,
        None => 0,
    }
}

/// Lay the subtree at `idx` out at absolute trie offset `base` using the
/// current assumed `widths`, then refine each child's width from the
/// position actually assigned. Returns the end offset of the subtree.
/// `changed` reports whether any width grew (the fixed-point signal).
fn layout_subtree(
    nodes: &[TrieNode],
    idx: usize,
    base: u32,
    widths: &mut Vec<usize>,
    assigned: &mut Vec<Option<u32>>,
    changed: &mut bool,
) -> u32 {
    assigned[idx] = Some(base);
    let node = &nodes[idx];
    let term_size = node_terminal_size(node);
    let header =
        uleb128_len(term_size) + term_size as usize + uleb128_len(node.children.len() as u64);
    let fields: usize = node
        .children
        .iter()
        .map(|(label, child)| 1 + label.len() + widths[*child])
        .sum();
    let record_end = base + (header + fields) as u32;

    // Children are laid out sequentially right after the parent's record.
    let mut child_cursor = record_end;
    let mut positions = Vec::with_capacity(node.children.len());
    for (_, child) in &node.children {
        positions.push(child_cursor);
        child_cursor = layout_subtree(nodes, *child, child_cursor, widths, assigned, changed);
    }

    // Refine widths from the assigned positions for the next pass.
    for ((_, child), position) in node.children.iter().zip(positions) {
        let needed = uleb128_len(position as u64);
        if needed > widths[*child] {
            widths[*child] = needed;
            *changed = true;
        }
    }

    child_cursor
}

// ─── test-visible decode helpers ────────────────────────────────────────────

/// Reference rebase decoder used by the unit tests (and useful for
/// diagnostics): decodes a rebase stream into `(segment, offset, type)`
/// triples exactly the way dyld walks it.
pub fn decode_rebase_stream_for_test(bytes: &[u8]) -> Vec<(u8, u64, u8)> {
    let mut result = Vec::new();
    let mut segment = 0u8;
    let mut offset = 0u64;
    let mut type_imm = 0u8;
    let mut i = 0usize;
    let read_uleb = |buf: &[u8], pos: &mut usize| -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = buf[*pos];
            *pos += 1;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    };
    while i < bytes.len() {
        let op = bytes[i];
        let imm = op & 0x0F;
        let opcode = op & 0xF0;
        i += 1;
        match opcode {
            0x00 => break, // DONE
            0x10 => type_imm = imm,
            0x30 => {
                segment = imm;
                offset = read_uleb(bytes, &mut i);
            }
            0x40 => {
                for _ in 0..imm {
                    result.push((segment, offset, type_imm));
                    offset += pointer_size_for_rebase(type_imm);
                }
            }
            0x50 => {
                let delta = read_uleb(bytes, &mut i);
                offset += delta;
            }
            0x60 => offset += (imm as u64) * 8,
            0x80 => {
                let times = read_uleb(bytes, &mut i);
                for _ in 0..times {
                    result.push((segment, offset, type_imm));
                    offset += pointer_size_for_rebase(type_imm);
                }
            }
            0x90 => {
                result.push((segment, offset, type_imm));
                let delta = read_uleb(bytes, &mut i);
                offset += delta;
            }
            0xC0 => {
                let times = read_uleb(bytes, &mut i);
                let skip = read_uleb(bytes, &mut i);
                for _ in 0..times {
                    offset += skip;
                    result.push((segment, offset, type_imm));
                    offset += pointer_size_for_rebase(type_imm);
                }
            }
            _ => panic!("unknown rebase opcode 0x{:02x}", op),
        }
    }
    result
}

fn pointer_size_for_rebase(type_imm: u8) -> u64 {
    if type_imm == REBASE_TYPE_TEXT_ABSOLUTE32 {
        4
    } else {
        8
    }
}

/// Reference bind decoder used by the unit tests: decodes a bind stream into
/// fully-qualified entries the way dyld's state machine does.
pub fn decode_bind_stream_for_test(bytes: &[u8]) -> Vec<(u8, u64, u8, u8, String, i64)> {
    let mut result = Vec::new();
    let mut segment = 0u8;
    let mut offset = 0u64;
    // dyld enters a bind stream with the fixup kind defaulted to POINTER;
    // lazy streams never state a type and still bind pointers.
    let mut type_imm = BIND_TYPE_POINTER;
    let mut ordinal: i64 = 0;
    let mut addend: i64 = 0;
    let mut symbol = String::new();
    let mut i = 0usize;
    let read_uleb = |buf: &[u8], pos: &mut usize| -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = buf[*pos];
            *pos += 1;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    };
    let read_sleb = |buf: &[u8], pos: &mut usize| -> i64 {
        let mut value = 0i64;
        let mut shift = 0;
        loop {
            let byte = buf[*pos];
            *pos += 1;
            value |= ((byte & 0x7f) as i64) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                if shift < 64 && (byte & 0x40) != 0 {
                    value |= -1i64 << shift;
                }
                return value;
            }
        }
    };
    while i < bytes.len() {
        let op = bytes[i];
        let imm = op & 0x0F;
        let opcode = op & 0xF0;
        i += 1;
        match opcode {
            0x00 => break, // DONE
            0x10 => ordinal = imm as i64,
            0x20 => ordinal = read_uleb(bytes, &mut i) as i64,
            0x30 => ordinal = imm as i64 - 16, // special ordinals encode as imm-16
            0x40 => {
                symbol.clear();
                while bytes[i] != 0 {
                    symbol.push(bytes[i] as char);
                    i += 1;
                }
                i += 1;
                // Trailing flags in the immediate are ignored here (no weak
                // imports are emitted today).
            }
            0x50 => type_imm = imm,
            0x60 => addend = read_sleb(bytes, &mut i),
            0x70 => {
                segment = imm;
                offset = read_uleb(bytes, &mut i);
            }
            0x80 => {
                let delta = read_uleb(bytes, &mut i);
                offset += delta;
            }
            0x90 => {
                result.push((
                    segment,
                    offset,
                    type_imm,
                    ordinal as u8,
                    symbol.clone(),
                    addend,
                ));
                offset += 8;
                addend = 0;
            }
            0xA0 => {
                result.push((
                    segment,
                    offset,
                    type_imm,
                    ordinal as u8,
                    symbol.clone(),
                    addend,
                ));
                offset += (imm as u64) * 8;
                addend = 0;
            }
            0xB0 => {
                result.push((
                    segment,
                    offset,
                    type_imm,
                    ordinal as u8,
                    symbol.clone(),
                    addend,
                ));
                let delta = read_uleb(bytes, &mut i);
                offset += delta;
                addend = 0;
            }
            0xC0 => {
                let times = read_uleb(bytes, &mut i);
                let skip = read_uleb(bytes, &mut i);
                for _ in 0..times {
                    offset += skip;
                    result.push((
                        segment,
                        offset,
                        type_imm,
                        ordinal as u8,
                        symbol.clone(),
                        addend,
                    ));
                    offset += 8;
                }
            }
            _ => panic!("unknown bind opcode 0x{:02x}", op),
        }
    }
    result
}

/// Reference export-trie decoder used by the unit tests: walks the trie and
/// returns every `(symbol, flags, address)` terminal, verifying child-offset
/// bookkeeping the way dyld's `forEachExport` does.
pub fn decode_export_trie_for_test(bytes: &[u8]) -> Vec<(String, u64, u64)> {
    fn read_uleb(buf: &[u8], pos: &mut usize) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = buf[*pos];
            *pos += 1;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    fn walk(bytes: &[u8], node: usize, prefix: &str, result: &mut Vec<(String, u64, u64)>) {
        let mut pos = node;
        let terminal_size = read_uleb(bytes, &mut pos) as usize;
        if terminal_size > 0 {
            let info_start = pos;
            let flags = read_uleb(bytes, &mut pos);
            let address = read_uleb(bytes, &mut pos);
            assert!(
                pos - info_start <= terminal_size,
                "terminal info overruns its declared size"
            );
            pos = info_start + terminal_size;
            result.push((prefix.to_string(), flags, address));
        }
        let child_count = read_uleb(bytes, &mut pos) as usize;
        let mut children = Vec::new();
        for _ in 0..child_count {
            let label_len = bytes[pos] as usize;
            pos += 1;
            let label: Vec<u8> = bytes[pos..pos + label_len].to_vec();
            pos += label_len;
            let child_offset = read_uleb(bytes, &mut pos) as usize;
            children.push((label, child_offset));
        }
        for (label, child_offset) in children {
            let child_prefix = format!("{}{}", prefix, String::from_utf8_lossy(&label));
            assert!(
                child_offset < bytes.len(),
                "child offset 0x{:x} outside trie",
                child_offset
            );
            walk(bytes, child_offset, &child_prefix, result);
        }
    }
    let mut result = Vec::new();
    walk(bytes, 0, "", &mut result);
    result.sort();
    result
}

// ─── unit tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uleb128_round_trip() {
        let values = [0u64, 1, 127, 128, 0x1234, u32::MAX as u64, u64::MAX];
        for value in values {
            let mut buf = Vec::new();
            write_uleb128(&mut buf, value);
            let mut pos = 0;
            let decoded = {
                let mut v = 0u64;
                let mut shift = 0;
                loop {
                    let b = buf[pos];
                    pos += 1;
                    v |= ((b & 0x7f) as u64) << shift;
                    if b & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }
                v
            };
            assert_eq!(decoded, value, "uleb128 round trip for {}", value);
            assert_eq!(pos, buf.len(), "uleb128 consumed whole encoding");
            assert_eq!(buf.len(), uleb128_len(value));
        }
    }

    #[test]
    fn test_sleb128_negative_round_trip() {
        for value in [0i64, 1, -1, 63, -64, 8191, -8192, i32::MIN as i64, -4200] {
            let mut buf = Vec::new();
            write_sleb128(&mut buf, value);
            let mut pos = 0;
            let decoded = {
                let mut v = 0i64;
                let mut shift = 0;
                loop {
                    let b = buf[pos];
                    pos += 1;
                    v |= ((b & 0x7f) as i64) << shift;
                    shift += 7;
                    if b & 0x80 == 0 {
                        if shift < 64 && (b & 0x40) != 0 {
                            v |= -1i64 << shift;
                        }
                        break;
                    }
                }
                v
            };
            assert_eq!(decoded, value, "sleb128 round trip for {}", value);
            assert_eq!(pos, buf.len());
        }
    }

    #[test]
    fn test_rebase_stream_round_trip_consecutive_pointers() {
        // Four consecutive __DATA pointer slots.
        let targets: Vec<RebaseTarget> = (0..4u64)
            .map(|i| RebaseTarget {
                segment_index: 1,
                offset: 0x1000 + i * 8,
                kind: RebaseKind::Pointer,
            })
            .collect();
        let bytes = encode_rebase_opcodes(&targets);
        let decoded = decode_rebase_stream_for_test(&bytes);
        assert_eq!(decoded.len(), 4);
        for (i, entry) in decoded.iter().enumerate() {
            assert_eq!(entry.0, 1);
            assert_eq!(entry.1, 0x1000 + (i as u64) * 8);
            assert_eq!(entry.2, REBASE_TYPE_POINTER);
        }
        // The run of 4 fits one DO_REBASE_IMM_TIMES chunk.
        assert!(
            bytes
                .windows(1)
                .any(|w| w[0] == REBASE_OPCODE_DO_REBASE_IMM_TIMES | 4),
            "expected a 4-count rebase chunk in {:02x?}",
            bytes
        );
    }

    #[test]
    fn test_rebase_stream_text_absolute32_and_gaps() {
        let targets = vec![
            RebaseTarget {
                segment_index: 0,
                offset: 0x200,
                kind: RebaseKind::TextAbsolute32,
            },
            RebaseTarget {
                segment_index: 1,
                offset: 0x3000,
                kind: RebaseKind::Pointer,
            },
            RebaseTarget {
                segment_index: 1,
                offset: 0x3010,
                kind: RebaseKind::Pointer,
            },
        ];
        let bytes = encode_rebase_opcodes(&targets);
        let decoded = decode_rebase_stream_for_test(&bytes);
        assert_eq!(
            decoded,
            vec![
                (0, 0x200, REBASE_TYPE_TEXT_ABSOLUTE32),
                (1, 0x3000, REBASE_TYPE_POINTER),
                (1, 0x3010, REBASE_TYPE_POINTER),
            ]
        );
    }

    #[test]
    fn test_rebase_stream_empty_is_done() {
        let bytes = encode_rebase_opcodes(&[]);
        assert_eq!(bytes, vec![REBASE_OPCODE_DONE]);
    }

    #[test]
    fn test_bind_stream_round_trip() {
        let targets = vec![
            BindTarget {
                segment_index: 1,
                offset: 0x4008,
                bind_type: BIND_TYPE_POINTER,
                ordinal: 1,
                symbol: "dyld_stub_binder".to_string(),
                addend: 0,
            },
            BindTarget {
                segment_index: 2,
                offset: 0x4020,
                bind_type: BIND_TYPE_POINTER,
                ordinal: 2,
                symbol: "_data_import".to_string(),
                addend: -16,
            },
        ];
        let bytes = encode_bind_opcodes(&targets);
        let decoded = decode_bind_stream_for_test(&bytes);
        assert_eq!(
            decoded,
            vec![
                (
                    1,
                    0x4008,
                    BIND_TYPE_POINTER,
                    1,
                    "dyld_stub_binder".to_string(),
                    0
                ),
                (
                    2,
                    0x4020,
                    BIND_TYPE_POINTER,
                    2,
                    "_data_import".to_string(),
                    -16
                ),
            ]
        );
        assert_eq!(*bytes.last().unwrap(), BIND_OPCODE_DONE);
    }

    #[test]
    fn test_lazy_bind_entries_are_self_contained_and_offset_addressable() {
        let targets = vec![
            LazyBindTarget {
                segment_index: 1,
                offset: 0x5000,
                ordinal: 1,
                symbol: "_puts".to_string(),
            },
            LazyBindTarget {
                segment_index: 1,
                offset: 0x5008,
                ordinal: 1,
                symbol: "_malloc".to_string(),
            },
            LazyBindTarget {
                segment_index: 1,
                offset: 0x5010,
                ordinal: 1,
                symbol: "_memcpy".to_string(),
            },
        ];
        let (bytes, offsets) = encode_lazy_bind_opcodes(&targets);
        assert_eq!(offsets[0], 0);
        // Every entry must decode standalone from its own byte span (the
        // exact parse dyld_stub_binder performs from a pushed offset — it
        // needs entry i complete without any earlier-entry state).
        for (i, target) in targets.iter().enumerate() {
            let span_start = offsets[i] as usize;
            let span_end = offsets
                .get(i + 1)
                .map(|next| *next as usize)
                .unwrap_or(bytes.len() - 1); // exclude the trailing DONE
            let entry = decode_bind_stream_for_test(&bytes[span_start..span_end]);
            assert_eq!(
                entry,
                vec![(
                    target.segment_index,
                    target.offset,
                    BIND_TYPE_POINTER,
                    target.ordinal,
                    target.symbol.clone(),
                    0
                )],
                "lazy entry {} must be self-contained within bytes 0x{:x}..0x{:x}",
                i,
                span_start,
                span_end
            );
        }
        assert_eq!(*bytes.last().unwrap(), BIND_OPCODE_DONE);
    }

    #[test]
    fn test_export_trie_decode_round_trip() {
        let exports = vec![
            MachOExport {
                name: "_adesh_add".to_string(),
                address: 0x1234,
            },
            MachOExport {
                name: "_adesh_mul".to_string(),
                address: 0x1300,
            },
            MachOExport {
                name: "_adesh_add_ten".to_string(),
                address: 0x1400,
            },
        ];
        let trie = build_export_trie(&exports);
        let decoded = decode_export_trie_for_test(&trie);
        assert_eq!(
            decoded,
            vec![
                (
                    "_adesh_add".to_string(),
                    EXPORT_SYMBOL_FLAGS_KIND_REGULAR,
                    0x1234
                ),
                (
                    "_adesh_add_ten".to_string(),
                    EXPORT_SYMBOL_FLAGS_KIND_REGULAR,
                    0x1400
                ),
                (
                    "_adesh_mul".to_string(),
                    EXPORT_SYMBOL_FLAGS_KIND_REGULAR,
                    0x1300
                ),
            ]
        );
    }

    #[test]
    fn test_export_trie_prefix_splitting() {
        // Exercises the edge-split path: insert "_ab", then "_a" (a prefix),
        // then "_abc". The trie must re-parent without losing terminals.
        let exports = vec![
            MachOExport {
                name: "_ab".to_string(),
                address: 1,
            },
            MachOExport {
                name: "_a".to_string(),
                address: 2,
            },
            MachOExport {
                name: "_abc".to_string(),
                address: 3,
            },
        ];
        let trie = build_export_trie(&exports);
        let decoded = decode_export_trie_for_test(&trie);
        assert_eq!(
            decoded,
            vec![
                ("_a".to_string(), EXPORT_SYMBOL_FLAGS_KIND_REGULAR, 2),
                ("_ab".to_string(), EXPORT_SYMBOL_FLAGS_KIND_REGULAR, 1),
                ("_abc".to_string(), EXPORT_SYMBOL_FLAGS_KIND_REGULAR, 3),
            ]
        );
    }

    #[test]
    fn test_export_trie_empty_is_valid_empty_root() {
        let trie = build_export_trie(&[]);
        assert_eq!(trie, vec![0u8, 0u8]);
        assert!(decode_export_trie_for_test(&trie).is_empty());
    }

    #[test]
    fn test_export_trie_large_offsets_use_wide_uleb() {
        // A trie big enough that child offsets exceed 127 forces the
        // fixed-point offset assignment to widen ulebs; the decoder must
        // still land every child.
        let mut exports = Vec::new();
        for i in 0..40u64 {
            exports.push(MachOExport {
                name: format!("_exported_symbol_number_{:03}", i),
                address: 0x1000 + i * 16,
            });
        }
        let trie = build_export_trie(&exports);
        let decoded = decode_export_trie_for_test(&trie);
        assert_eq!(decoded.len(), 40);
        let mut names: Vec<String> = decoded.iter().map(|(n, _, _)| n.clone()).collect();
        names.sort();
        let mut expected: Vec<String> = (0..40u64)
            .map(|i| format!("_exported_symbol_number_{:03}", i))
            .collect();
        expected.sort();
        assert_eq!(names, expected);
        for export in &exports {
            assert!(
                decoded
                    .iter()
                    .any(|(n, _, a)| *n == export.name && *a == export.address),
                "missing trie entry for {}",
                export.name
            );
        }
    }
}
