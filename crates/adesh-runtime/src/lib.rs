//! Standalone Native Runtime for Adesh Programming Language
//!
//! Provides self-contained C ABI exports for AOT compiled binaries.
//! Includes rich pretty printing, composite structures (objects, arrays, tuples, sets),
//! ARC reference counting, tracked allocations, and standard builtins.

pub mod abi;
pub mod allocator;
pub mod threading;

pub use abi::*;
pub use allocator::*;
pub use threading::*;

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// ============================================================================
// Runtime Value Definition
// ============================================================================

#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    String(String),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    F32(f32),
    F64(f64),
    Array(Vec<RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    Set(Vec<RuntimeValue>),
    Object(BTreeMap<String, RuntimeValue>),
    Ref(Box<RuntimeValue>),
    Function(usize),
    VecDeque(std::collections::VecDeque<RuntimeValue>),
    HashSet(std::collections::HashSet<String>),
    BTreeMap(std::collections::BTreeMap<String, RuntimeValue>),
    BinaryHeap(std::collections::BinaryHeap<OrderedValue>),
    PriorityQueue(std::collections::BinaryHeap<PriorityItem>),
    BitSet(Vec<bool>),
    RingBuffer {
        buffer: Vec<RuntimeValue>,
        head: usize,
        tail: usize,
        count: usize,
        cap: usize,
    },
}

impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Null, RuntimeValue::Null) => true,
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
            (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a == b,
            (RuntimeValue::Float(a), RuntimeValue::Float(b)) => a == b,
            (RuntimeValue::Char(a), RuntimeValue::Char(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a == b,
            (RuntimeValue::Tuple(a), RuntimeValue::Tuple(b)) => a == b,
            (RuntimeValue::Set(a), RuntimeValue::Set(b)) => a == b,
            (RuntimeValue::Object(a), RuntimeValue::Object(b)) => a == b,
            (RuntimeValue::Function(a), RuntimeValue::Function(b)) => a == b,
            (RuntimeValue::VecDeque(a), RuntimeValue::VecDeque(b)) => a == b,
            (RuntimeValue::HashSet(a), RuntimeValue::HashSet(b)) => a == b,
            (RuntimeValue::BTreeMap(a), RuntimeValue::BTreeMap(b)) => a == b,
            (RuntimeValue::BitSet(a), RuntimeValue::BitSet(b)) => a == b,
            _ => {
                if let (Some(a), Some(b)) = (self.as_i64(), other.as_i64()) {
                    a == b
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderedValue(pub RuntimeValue);

impl PartialEq for OrderedValue {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for OrderedValue {}

impl PartialOrd for OrderedValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.0, &other.0) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.cmp(b),
            (RuntimeValue::Float(a), RuntimeValue::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a.cmp(b),
            _ => {
                let na = self.0.as_i64();
                let nb = other.0.as_i64();
                match (na, nb) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    _ => self.0.as_string().cmp(&other.0.as_string()),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PriorityItem {
    pub item: RuntimeValue,
    pub priority: i64,
}

impl PartialEq for PriorityItem {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for PriorityItem {}

impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl RuntimeValue {
    pub fn as_string(&self) -> String {
        match self {
            RuntimeValue::Null => "null".to_string(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::Int(i) => i.to_string(),
            RuntimeValue::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{:.1}", f)
                } else {
                    f.to_string()
                }
            }
            RuntimeValue::Char(c) => c.to_string(),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::U8(n) => n.to_string(),
            RuntimeValue::U16(n) => n.to_string(),
            RuntimeValue::U32(n) => n.to_string(),
            RuntimeValue::U64(n) => n.to_string(),
            RuntimeValue::U128(n) => n.to_string(),
            RuntimeValue::I8(n) => n.to_string(),
            RuntimeValue::I16(n) => n.to_string(),
            RuntimeValue::I32(n) => n.to_string(),
            RuntimeValue::I64(n) => n.to_string(),
            RuntimeValue::I128(n) => n.to_string(),
            RuntimeValue::F32(n) => n.to_string(),
            RuntimeValue::F64(n) => n.to_string(),
            RuntimeValue::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| {
                        if let RuntimeValue::String(s) = v {
                            format!("\"{}\"", s)
                        } else {
                            v.as_string()
                        }
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Tuple(tup) => {
                let items: Vec<String> = tup
                    .iter()
                    .map(|v| {
                        if let RuntimeValue::String(s) = v {
                            format!("\"{}\"", s)
                        } else {
                            v.as_string()
                        }
                    })
                    .collect();
                format!("({})", items.join(", "))
            }
            RuntimeValue::Set(set) => {
                let items: Vec<String> = set
                    .iter()
                    .map(|v| {
                        if let RuntimeValue::String(s) = v {
                            format!("\"{}\"", s)
                        } else {
                            v.as_string()
                        }
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        if let RuntimeValue::String(s) = v {
                            format!("{}: \"{}\"", k, s)
                        } else {
                            format!("{}: {}", k, v.as_string())
                        }
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Ref(inner) => inner.as_string(),
            RuntimeValue::Function(ptr) => format!("<function@0x{:x}>", ptr),
            RuntimeValue::VecDeque(d) => {
                let items: Vec<String> = d.iter().map(|v| v.as_string()).collect();
                format!("VecDeque([{}])", items.join(", "))
            }
            RuntimeValue::HashSet(s) => {
                let items: Vec<String> = s.iter().cloned().collect();
                format!("HashSet({{{}}})", items.join(", "))
            }
            RuntimeValue::BTreeMap(m) => {
                let items: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.as_string()))
                    .collect();
                format!("BTreeMap({{{}}})", items.join(", "))
            }
            RuntimeValue::BinaryHeap(_) => "BinaryHeap".to_string(),
            RuntimeValue::PriorityQueue(_) => "PriorityQueue".to_string(),
            RuntimeValue::BitSet(_) => "BitSet".to_string(),
            RuntimeValue::RingBuffer {
                buffer: _,
                count,
                cap,
                ..
            } => format!("RingBuffer(count={}, cap={})", count, cap),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            RuntimeValue::Null => "null",
            RuntimeValue::Bool(_) => "bool",
            RuntimeValue::Int(_) | RuntimeValue::I64(_) => "i64",
            RuntimeValue::I32(_) => "i32",
            RuntimeValue::I16(_) => "i16",
            RuntimeValue::I8(_) => "i8",
            RuntimeValue::I128(_) => "i128",
            RuntimeValue::U64(_) => "u64",
            RuntimeValue::U32(_) => "u32",
            RuntimeValue::U16(_) => "u16",
            RuntimeValue::U8(_) => "u8",
            RuntimeValue::U128(_) => "u128",
            RuntimeValue::Float(_) | RuntimeValue::F64(_) => "f64",
            RuntimeValue::F32(_) => "f32",
            RuntimeValue::Char(_) => "char",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Array(_) => "array",
            RuntimeValue::Tuple(_) => "tuple",
            RuntimeValue::Set(_) => "set",
            RuntimeValue::Object(_) => "object",
            RuntimeValue::Ref(inner) => inner.type_name(),
            RuntimeValue::Function(_) => "function",
            RuntimeValue::VecDeque(_) => "VecDeque",
            RuntimeValue::HashSet(_) => "HashSet",
            RuntimeValue::BTreeMap(_) => "BTreeMap",
            RuntimeValue::BinaryHeap(_) => "BinaryHeap",
            RuntimeValue::PriorityQueue(_) => "PriorityQueue",
            RuntimeValue::BitSet(_) => "BitSet",
            RuntimeValue::RingBuffer { .. } => "RingBuffer",
        }
    }

    pub fn size_of_value(&self) -> usize {
        match self {
            RuntimeValue::Null => 0,
            RuntimeValue::Bool(_) | RuntimeValue::U8(_) | RuntimeValue::I8(_) => 1,
            RuntimeValue::U16(_) | RuntimeValue::I16(_) => 2,
            RuntimeValue::U32(_)
            | RuntimeValue::I32(_)
            | RuntimeValue::F32(_)
            | RuntimeValue::Char(_) => 4,
            RuntimeValue::Int(_)
            | RuntimeValue::Float(_)
            | RuntimeValue::U64(_)
            | RuntimeValue::I64(_)
            | RuntimeValue::F64(_)
            | RuntimeValue::Function(_) => 8,
            RuntimeValue::U128(_) | RuntimeValue::I128(_) => 16,
            RuntimeValue::String(s) => s.len(),
            RuntimeValue::Array(arr) => 24 + arr.iter().map(|v| v.size_of_value()).sum::<usize>(),
            RuntimeValue::Tuple(tup) => 24 + tup.iter().map(|v| v.size_of_value()).sum::<usize>(),
            RuntimeValue::Set(set) => 24 + set.iter().map(|v| v.size_of_value()).sum::<usize>(),
            RuntimeValue::Object(obj) => {
                32 + obj
                    .iter()
                    .map(|(k, v)| k.len() + v.size_of_value())
                    .sum::<usize>()
            }
            RuntimeValue::Ref(inner) => 8 + inner.size_of_value(),
            RuntimeValue::VecDeque(d) => 24 + d.iter().map(|v| v.size_of_value()).sum::<usize>(),
            RuntimeValue::HashSet(s) => 32 + s.iter().map(|k| k.len()).sum::<usize>(),
            RuntimeValue::BTreeMap(m) => {
                32 + m
                    .iter()
                    .map(|(k, v)| k.len() + v.size_of_value())
                    .sum::<usize>()
            }
            RuntimeValue::BinaryHeap(h) => 24 + h.len() * 8,
            RuntimeValue::PriorityQueue(pq) => 24 + pq.len() * 16,
            RuntimeValue::BitSet(bs) => 24 + (bs.len() + 7) / 8,
            RuntimeValue::RingBuffer { cap, .. } => 32 + cap * 8,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        match self {
            RuntimeValue::Int(i) | RuntimeValue::I64(i) => Some(*i as usize),
            RuntimeValue::I32(i) => Some(*i as usize),
            RuntimeValue::I16(i) => Some(*i as usize),
            RuntimeValue::I8(i) => Some(*i as usize),
            RuntimeValue::U64(u) => Some(*u as usize),
            RuntimeValue::U32(u) => Some(*u as usize),
            RuntimeValue::U16(u) => Some(*u as usize),
            RuntimeValue::U8(u) => Some(*u as usize),
            RuntimeValue::Float(f) | RuntimeValue::F64(f) => Some(*f as usize),
            RuntimeValue::F32(f) => Some(*f as usize),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            RuntimeValue::Int(i) | RuntimeValue::I64(i) => Some(*i),
            RuntimeValue::I32(i) => Some(*i as i64),
            RuntimeValue::I16(i) => Some(*i as i64),
            RuntimeValue::I8(i) => Some(*i as i64),
            RuntimeValue::U64(u) => Some(*u as i64),
            RuntimeValue::U32(u) => Some(*u as i64),
            RuntimeValue::U16(u) => Some(*u as i64),
            RuntimeValue::U8(u) => Some(*u as i64),
            RuntimeValue::Float(f) | RuntimeValue::F64(f) => Some(*f as i64),
            RuntimeValue::F32(f) => Some(*f as i64),
            RuntimeValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_fn_ptr(&self) -> Option<usize> {
        match self {
            RuntimeValue::Function(ptr) => Some(*ptr),
            RuntimeValue::Int(ptr) | RuntimeValue::I64(ptr) => {
                if *ptr != 0 {
                    Some(*ptr as usize)
                } else {
                    None
                }
            }
            RuntimeValue::U64(ptr) => {
                if *ptr != 0 {
                    Some(*ptr as usize)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            RuntimeValue::Null => false,
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) | RuntimeValue::I64(i) => *i != 0,
            RuntimeValue::Float(f) | RuntimeValue::F64(f) => *f != 0.0 && !f.is_nan(),
            RuntimeValue::String(s) => !s.is_empty(),
            RuntimeValue::Array(a) => !a.is_empty(),
            _ => true,
        }
    }
}

// ============================================================================
// Handle Storage Management
// ============================================================================

static VALUE_STORE: Mutex<Option<BTreeMap<u64, RuntimeValue>>> = Mutex::new(None);
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(100);

fn get_store_guard() -> std::sync::MutexGuard<'static, Option<BTreeMap<u64, RuntimeValue>>> {
    let mut guard = VALUE_STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(BTreeMap::new());
    }
    guard
}

pub fn aot_store_value(value: RuntimeValue) -> u64 {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut guard = get_store_guard();
    if let Some(ref mut map) = *guard {
        map.insert(handle, value);
    }
    handle
}

pub fn aot_update_value(handle: u64, value: RuntimeValue) {
    let mut guard = get_store_guard();
    if let Some(ref mut map) = *guard {
        map.insert(handle, value);
    }
}

pub fn aot_get_value(handle: u64) -> Option<RuntimeValue> {
    let guard = get_store_guard();
    guard.as_ref().and_then(|map| map.get(&handle).cloned())
}

pub fn aot_remove_value(handle: u64) -> Option<RuntimeValue> {
    let mut guard = get_store_guard();
    guard.as_mut().and_then(|map| map.remove(&handle))
}

pub fn get_string_val(ptr_or_handle: u64) -> Option<String> {
    if ptr_or_handle == 0 {
        return None;
    }
    if let Some(v) = aot_get_value(ptr_or_handle) {
        return match v {
            RuntimeValue::String(s) => Some(s),
            _ => None,
        };
    }
    if ptr_or_handle > 0x10000 {
        unsafe {
            if let Ok(c_str) = CStr::from_ptr(ptr_or_handle as *const c_char).to_str() {
                return Some(c_str.to_string());
            }
        }
    }
    None
}

pub fn unpack_aot_arg(raw: u64) -> RuntimeValue {
    if let Some(v) = aot_get_value(raw) {
        return v;
    }
    if raw > 0x10000 {
        if let Some(s) = get_string_val(raw) {
            return RuntimeValue::String(s);
        }
    }
    RuntimeValue::Int(raw as i64)
}

// ============================================================================
// Pretty Printing Engine
// ============================================================================

#[derive(Clone, Copy)]
pub struct ColorScheme {
    pub key: &'static str,
    pub string: &'static str,
    pub number: &'static str,
    pub boolean: &'static str,
    pub null: &'static str,
    pub bracket: &'static str,
    pub colon: &'static str,
    pub comma: &'static str,
    pub reset: &'static str,
    pub type_hint: &'static str,
}

impl ColorScheme {
    pub const fn default() -> Self {
        ColorScheme {
            key: "\x1b[38;2;156;220;254m",      // Light blue for keys
            string: "\x1b[38;2;206;145;120m",   // Peach/orange for strings
            number: "\x1b[38;2;181;206;168m",   // Light green for numbers
            boolean: "\x1b[38;2;86;156;214m",   // Blue for booleans
            null: "\x1b[38;2;128;128;128m",     // Gray for null
            bracket: "\x1b[38;2;212;212;212m",  // Light gray for brackets
            colon: "\x1b[38;2;212;212;212m",    // Light gray for colons
            comma: "\x1b[38;2;212;212;212m",    // Light gray for commas
            type_hint: "\x1b[38;2;78;201;176m", // Teal for type hints
            reset: "\x1b[0m",
        }
    }

    pub const fn simple() -> Self {
        ColorScheme {
            key: "\x1b[36m",
            string: "\x1b[33m",
            number: "\x1b[32m",
            boolean: "\x1b[35m",
            null: "\x1b[90m",
            bracket: "\x1b[37m",
            colon: "\x1b[37m",
            comma: "\x1b[37m",
            type_hint: "\x1b[96m",
            reset: "\x1b[0m",
        }
    }

    pub const fn none() -> Self {
        ColorScheme {
            key: "",
            string: "",
            number: "",
            boolean: "",
            null: "",
            bracket: "",
            colon: "",
            comma: "",
            type_hint: "",
            reset: "",
        }
    }
}

#[derive(Clone)]
pub struct PrettyPrintOptions {
    pub indent_str: String,
    pub max_depth: usize,
    pub colors: ColorScheme,
    pub show_types: bool,
    pub expand_objects: bool,
    pub show_indices: bool,
    pub align_values: bool,
}

impl Default for PrettyPrintOptions {
    fn default() -> Self {
        PrettyPrintOptions {
            indent_str: "  ".to_string(),
            max_depth: 10,
            colors: ColorScheme::default(),
            show_types: true,
            expand_objects: true,
            show_indices: false,
            align_values: true,
        }
    }
}

impl PrettyPrintOptions {
    pub fn no_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::none(),
            ..Default::default()
        }
    }

    pub fn simple_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::simple(),
            ..Default::default()
        }
    }

    pub fn compact() -> Self {
        PrettyPrintOptions {
            show_types: false,
            expand_objects: true,
            show_indices: false,
            align_values: false,
            ..Default::default()
        }
    }
}

fn infer_integer_hint_from_number(n: f64) -> Option<&'static str> {
    if !n.is_finite() {
        return None;
    }
    if n.fract() != 0.0 {
        let f32_val = n as f32 as f64;
        if (f32_val - n).abs() < 1e-6 && n.abs() <= 1e7 {
            Some("f32")
        } else {
            Some("f64")
        }
    } else if n >= 0.0 {
        if n <= u8::MAX as f64 {
            Some("u8")
        } else if n <= u16::MAX as f64 {
            Some("u16")
        } else if n <= u32::MAX as f64 {
            Some("u32")
        } else {
            Some("u64")
        }
    } else if n >= i8::MIN as f64 {
        Some("i8")
    } else if n >= i16::MIN as f64 {
        Some("i16")
    } else if n >= i32::MIN as f64 {
        Some("i32")
    } else {
        Some("i64")
    }
}

fn pretty_print(value: &RuntimeValue, options: &PrettyPrintOptions) -> String {
    let mut output = String::new();
    pretty_print_inner(value, options, &mut output, 0, 0);
    output
}

fn pretty_print_inner(
    value: &RuntimeValue,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    if depth >= options.max_depth {
        let _ = write!(
            output,
            "{}<max depth>{}",
            options.colors.null, options.colors.reset
        );
        return;
    }

    match value {
        RuntimeValue::Null => {
            let _ = write!(
                output,
                "{}null{}",
                options.colors.null, options.colors.reset
            );
        }
        RuntimeValue::Bool(b) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.boolean, b, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨bool⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Int(n) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, n, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨i64⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Float(n) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, n, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨f64⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Char(c) => {
            let _ = write!(
                output,
                "{}'{}'{}",
                options.colors.string, c, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨char⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::String(s) => {
            let _ = write!(
                output,
                "{}\"{}\"{}",
                options.colors.string, s, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨string⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::U8(n) => print_typed_num(output, n, "u8", options),
        RuntimeValue::U16(n) => print_typed_num(output, n, "u16", options),
        RuntimeValue::U32(n) => print_typed_num(output, n, "u32", options),
        RuntimeValue::U64(n) => print_typed_num(output, n, "u64", options),
        RuntimeValue::U128(n) => print_typed_num(output, n, "u128", options),
        RuntimeValue::I8(n) => print_typed_num(output, n, "i8", options),
        RuntimeValue::I16(n) => print_typed_num(output, n, "i16", options),
        RuntimeValue::I32(n) => print_typed_num(output, n, "i32", options),
        RuntimeValue::I64(n) => print_typed_num(output, n, "i64", options),
        RuntimeValue::I128(n) => print_typed_num(output, n, "i128", options),
        RuntimeValue::F32(n) => print_typed_num(output, n, "f32", options),
        RuntimeValue::F64(n) => print_typed_num(output, n, "f64", options),
        RuntimeValue::Array(arr) => {
            if arr.is_empty() {
                let _ = write!(
                    output,
                    "{}[]{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨array⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_array_impl(arr, options, output, depth, current_indent);
            }
        }
        RuntimeValue::Tuple(tup) => {
            let _ = write!(
                output,
                "{}({}\n",
                options.colors.bracket, options.colors.reset
            );
            let new_indent = current_indent + 1;
            for (i, item) in tup.iter().enumerate() {
                for _ in 0..new_indent {
                    output.push_str(&options.indent_str);
                }
                pretty_print_inner(item, options, output, depth + 1, new_indent);
                if i < tup.len() - 1 {
                    let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
                }
                output.push('\n');
            }
            for _ in 0..current_indent {
                output.push_str(&options.indent_str);
            }
            let _ = write!(
                output,
                "{}){}",
                options.colors.bracket, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨tuple[{}]⟩{}",
                    options.colors.type_hint,
                    tup.len(),
                    options.colors.reset
                );
            }
        }
        RuntimeValue::Set(set) => {
            let _ = write!(
                output,
                "{}{{{}\n",
                options.colors.bracket, options.colors.reset
            );
            let new_indent = current_indent + 1;
            for (i, item) in set.iter().enumerate() {
                for _ in 0..new_indent {
                    output.push_str(&options.indent_str);
                }
                pretty_print_inner(item, options, output, depth + 1, new_indent);
                if i < set.len() - 1 {
                    let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
                }
                output.push('\n');
            }
            for _ in 0..current_indent {
                output.push_str(&options.indent_str);
            }
            let _ = write!(
                output,
                "{}}}{}",
                options.colors.bracket, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨set[{}]⟩{}",
                    options.colors.type_hint,
                    set.len(),
                    options.colors.reset
                );
            }
        }
        RuntimeValue::Object(obj) => {
            if obj.is_empty() {
                let _ = write!(
                    output,
                    "{}{{}}{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨object⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_object_impl(obj, options, output, depth, current_indent);
            }
        }
        RuntimeValue::Ref(inner) => {
            pretty_print_inner(inner, options, output, depth, current_indent);
        }
        RuntimeValue::Function(ptr) => {
            let _ = write!(
                output,
                "{}<function@0x{:x}>{}",
                options.colors.type_hint, ptr, options.colors.reset
            );
        }
        RuntimeValue::VecDeque(d) => {
            let items: Vec<RuntimeValue> = d.iter().cloned().collect();
            print_array_impl(&items, options, output, depth, current_indent);
        }
        RuntimeValue::HashSet(s) => {
            let items: Vec<RuntimeValue> =
                s.iter().map(|k| RuntimeValue::String(k.clone())).collect();
            print_array_impl(&items, options, output, depth, current_indent);
        }
        RuntimeValue::BTreeMap(m) => {
            print_object_impl(m, options, output, depth, current_indent);
        }
        RuntimeValue::BinaryHeap(_) => {
            let _ = write!(
                output,
                "{}BinaryHeap{}",
                options.colors.type_hint, options.colors.reset
            );
        }
        RuntimeValue::PriorityQueue(_) => {
            let _ = write!(
                output,
                "{}PriorityQueue{}",
                options.colors.type_hint, options.colors.reset
            );
        }
        RuntimeValue::BitSet(bs) => {
            let _ = write!(
                output,
                "{}BitSet({:?}){}",
                options.colors.type_hint, bs, options.colors.reset
            );
        }
        RuntimeValue::RingBuffer { buffer, .. } => {
            print_array_impl(buffer, options, output, depth, current_indent);
        }
    }
}

fn print_typed_num<T: std::fmt::Display>(
    output: &mut String,
    val: &T,
    type_name: &str,
    options: &PrettyPrintOptions,
) {
    let _ = write!(
        output,
        "{}{}{}",
        options.colors.number, val, options.colors.reset
    );
    if options.show_types {
        let _ = write!(
            output,
            " {}⟨{}⟩{}",
            options.colors.type_hint, type_name, options.colors.reset
        );
    }
}

fn print_array_impl(
    arr: &[RuntimeValue],
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}[{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    for (i, item) in arr.iter().enumerate() {
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }
        if options.show_indices {
            let _ = write!(
                output,
                "{}{}:{} ",
                options.colors.type_hint, i, options.colors.reset
            );
        }
        pretty_print_inner(item, options, output, depth + 1, new_indent);
        if i < arr.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}]{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨array[{}]⟩{}",
            options.colors.type_hint,
            arr.len(),
            options.colors.reset
        );
    }
}

fn print_object_impl(
    obj: &BTreeMap<String, RuntimeValue>,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}{{{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    let mut keys: Vec<_> = obj.keys().collect();
    keys.sort();

    let max_key_len = if options.align_values {
        keys.iter().map(|k| k.len()).max().unwrap_or(0)
    } else {
        0
    };

    for (i, key) in keys.iter().enumerate() {
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        let _ = write!(output, "{}", options.colors.key);
        if options.align_values {
            let _ = write!(output, "{:<width$}", key, width = max_key_len);
        } else {
            let _ = write!(output, "{}", key);
        }
        let _ = write!(output, "{}", options.colors.reset);

        let _ = write!(output, "{}: {}", options.colors.colon, options.colors.reset);

        if let Some(value) = obj.get(*key) {
            pretty_print_inner(value, options, output, depth + 1, new_indent);
        }

        if i < keys.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}}}{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨object⟩{}",
            options.colors.type_hint, options.colors.reset
        );
    }
}

// ============================================================================
// C-ABI Exports for Object, Array, Value Construction
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_object(args_ptr: *const u64, arg_count: usize) -> u64 {
    if arg_count == 0 {
        return aot_store_value(RuntimeValue::Object(BTreeMap::new()));
    }
    if args_ptr.is_null() || arg_count % 2 != 0 {
        return 0;
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut obj = BTreeMap::new();

    for chunk in args.chunks(2) {
        if chunk.len() == 2 {
            let key_handle = chunk[0];
            let val_handle = chunk[1];

            let key_str = if let Some(s) = get_string_val(key_handle) {
                s
            } else if let Some(RuntimeValue::String(s)) = aot_get_value(key_handle) {
                s
            } else {
                continue;
            };

            let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
            obj.insert(key_str, val);
        }
    }

    aot_store_value(RuntimeValue::Object(obj))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_set_field(obj_handle: u64, field_handle: u64, val_handle: u64) -> u64 {
    let field_name = match get_string_val(field_handle) {
        Some(s) => s,
        None => return 0,
    };
    let value = aot_get_value(val_handle).unwrap_or(RuntimeValue::Null);

    match aot_get_value(obj_handle) {
        Some(RuntimeValue::Object(mut obj)) => {
            obj.insert(field_name, value);
            aot_store_value(RuntimeValue::Object(obj))
        }
        _ => 0,
    }
}

pub fn runtime_value_to_raw_or_handle(val: RuntimeValue) -> u64 {
    match val {
        RuntimeValue::Int(i) | RuntimeValue::I64(i) => i as u64,
        RuntimeValue::I32(i) => i as i64 as u64,
        RuntimeValue::I16(i) => i as i64 as u64,
        RuntimeValue::I8(i) => i as i64 as u64,
        RuntimeValue::U64(u) => u,
        RuntimeValue::U32(u) => u as u64,
        RuntimeValue::U16(u) => u as u64,
        RuntimeValue::U8(u) => u as u64,
        RuntimeValue::Bool(b) => {
            if b {
                1
            } else {
                0
            }
        }
        RuntimeValue::Char(c) => c as u64,
        _ => aot_store_value(val),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_get_field(obj_handle: u64, field_handle: u64) -> u64 {
    let field_name = match get_string_val(field_handle) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Null),
    };

    let resolved = aot_get_value(obj_handle).unwrap_or(RuntimeValue::Null);
    match resolved {
        RuntimeValue::Object(obj) => {
            let val = obj.get(&field_name).cloned().unwrap_or(RuntimeValue::Null);
            runtime_value_to_raw_or_handle(val)
        }
        RuntimeValue::Array(arr) => {
            let val = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.capacity() as i64),
                _ => RuntimeValue::Null,
            };
            runtime_value_to_raw_or_handle(val)
        }
        RuntimeValue::String(s) => {
            let val = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(s.len() as i64),
                _ => RuntimeValue::Null,
            };
            runtime_value_to_raw_or_handle(val)
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_array(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Array(Vec::new()));
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);

    for &handle in args {
        let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
        arr.push(val);
    }

    aot_store_value(RuntimeValue::Array(arr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_string(string_ptr: *const c_char) -> u64 {
    if string_ptr.is_null() {
        return aot_store_value(RuntimeValue::String(String::new()));
    }
    let s = unsafe {
        CStr::from_ptr(string_ptr)
            .to_str()
            .unwrap_or("")
            .to_string()
    };
    aot_store_value(RuntimeValue::String(s))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_wrap_ptr(value: u64) -> u64 {
    if value == 0 {
        return aot_store_value(RuntimeValue::Null);
    }
    if aot_get_value(value).is_some() {
        return value;
    }
    let signed = value as i64;
    if signed < 0 || value < 0x10000 || value >= 0x0000_8000_0000_0000 {
        return aot_store_value(RuntimeValue::Int(signed));
    }
    unsafe { aot_make_string(value as *const c_char) }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u8(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U8(value as u8))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u16(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U16(value as u16))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u32(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U32(value as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u64(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U64(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i8(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I8(value as i8))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i16(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I16(value as i16))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i32(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I32(value as i32))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i64(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Int(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f32(value_bits: u64) -> u64 {
    let f = f32::from_bits(value_bits as u32);
    aot_store_value(RuntimeValue::F32(f))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f64(value_bits: u64) -> u64 {
    let f = f64::from_bits(value_bits);
    aot_store_value(RuntimeValue::Float(f))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_bool(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Bool(value != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_char(value: u64) -> u64 {
    let ch = char::from_u32(value as u32).unwrap_or('\0');
    aot_store_value(RuntimeValue::Char(ch))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_tuple(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Tuple(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle)));
    }
    aot_store_value(RuntimeValue::Tuple(arr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_set(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Set(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle)));
    }
    aot_store_value(RuntimeValue::Set(arr))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_null() -> u64 {
    aot_store_value(RuntimeValue::Null)
}

// ============================================================================
// Pretty Printing & Console Output Exports
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_value_pretty(handle: u64, mode: i64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    let opts = match mode {
        2 => PrettyPrintOptions::compact(),
        3 => PrettyPrintOptions::simple_color(),
        1 => PrettyPrintOptions::default(),
        _ => {
            print!("{}", val.as_string());
            let _ = std::io::stdout().flush();
            return 0;
        }
    };
    let output = pretty_print(&val, &opts);
    print!("{}", output);
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_newline() -> u64 {
    println!();
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_space() -> u64 {
    print!(" ");
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i8(value: i8, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i16(value: i16, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i32(value: i32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i64(value: i64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u8(value: u8, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u16(value: u16, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u32(value: u32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u64(value: u64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_f32(value: f32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_f64(value: f64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_str(string_ptr: i64, newline: i64) -> u64 {
    if string_ptr == 0 {
        return 0;
    }
    unsafe {
        let ptr = string_ptr as *const c_char;
        if let Ok(s) = CStr::from_ptr(ptr).to_str() {
            if newline != 0 {
                println!("{}", s);
            } else {
                print!("{}", s);
            }
        }
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_bool(value: i64, newline: i64) -> u64 {
    let bool_val = value != 0;
    if newline != 0 {
        println!("{}", bool_val);
    } else {
        print!("{}", bool_val);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_null(newline: i64) -> u64 {
    if newline != 0 {
        println!("null");
    } else {
        print!("null");
    }
    let _ = std::io::stdout().flush();
    0
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Some((r, g, b));
        }
    } else if hex.len() == 3 {
        let chars: Vec<char> = hex.chars().collect();
        let r_str = format!("{}{}", chars[0], chars[0]);
        let g_str = format!("{}{}", chars[1], chars[1]);
        let b_str = format!("{}{}", chars[2], chars[2]);
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&r_str, 16),
            u8::from_str_radix(&g_str, 16),
            u8::from_str_radix(&b_str, 16),
        ) {
            return Some((r, g, b));
        }
    }
    None
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_print_with_options(
    values_ptr: *const u64,
    values_count: i64,
    options_handle: u64,
) -> u64 {
    if values_count <= 0 && options_handle == 0 {
        return 0;
    }
    let count = values_count.max(0) as usize;
    let handles = if !values_ptr.is_null() && count > 0 {
        unsafe { std::slice::from_raw_parts(values_ptr, count) }
    } else {
        &[]
    };

    // Check options object (passed explicitly or as the last argument in handles)
    let mut pretty_mode = 0i64;
    let mut sep = " ".to_string();
    let mut end = "\n".to_string();
    let mut file_path: Option<String> = None;
    let mut color: Option<String> = None;
    let mut background: Option<String> = None;
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut strikethrough = false;
    let mut flush = false;
    let mut has_options = false;

    let mut check_opts_handle = options_handle;
    if check_opts_handle == 0 || aot_get_value(check_opts_handle).is_none() {
        if let Some(&last_h) = handles.last() {
            if let Some(RuntimeValue::Object(ref obj)) = aot_get_value(last_h) {
                if obj.contains_key("pretty")
                    || obj.contains_key("sep")
                    || obj.contains_key("end")
                    || obj.contains_key("file")
                    || obj.contains_key("color")
                    || obj.contains_key("background")
                    || obj.contains_key("underline")
                    || obj.contains_key("bold")
                    || obj.contains_key("italic")
                    || obj.contains_key("strikethrough")
                    || obj.contains_key("flush")
                {
                    check_opts_handle = last_h;
                }
            }
        }
    }

    if let Some(RuntimeValue::Object(ref opts_obj)) = aot_get_value(check_opts_handle) {
        has_options = opts_obj.contains_key("pretty")
            || opts_obj.contains_key("sep")
            || opts_obj.contains_key("end")
            || opts_obj.contains_key("file")
            || opts_obj.contains_key("color")
            || opts_obj.contains_key("background")
            || opts_obj.contains_key("underline")
            || opts_obj.contains_key("bold")
            || opts_obj.contains_key("italic")
            || opts_obj.contains_key("strikethrough")
            || opts_obj.contains_key("flush");

        if let Some(pv) = opts_obj.get("pretty") {
            pretty_mode = match pv {
                RuntimeValue::Bool(true) => 1,
                RuntimeValue::Int(1) => 1,
                RuntimeValue::String(s) => match s.as_str() {
                    "compact" => 2,
                    "simple" => 3,
                    "full" | "true" => 1,
                    _ => 0,
                },
                _ => 0,
            };
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("sep") {
            sep = s.clone();
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("end") {
            end = s.clone();
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("file") {
            file_path = Some(s.clone());
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("color") {
            color = Some(s.clone());
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("background") {
            background = Some(s.clone());
        }
        if let Some(RuntimeValue::Bool(b)) = opts_obj.get("bold") {
            bold = *b;
        }
        if let Some(RuntimeValue::Bool(b)) = opts_obj.get("italic") {
            italic = *b;
        }
        if let Some(RuntimeValue::Bool(b)) = opts_obj.get("underline") {
            underline = *b;
        }
        if let Some(RuntimeValue::Bool(b)) = opts_obj.get("strikethrough") {
            strikethrough = *b;
        }
        if let Some(RuntimeValue::Bool(b)) = opts_obj.get("flush") {
            flush = *b;
        }
    }

    let effective_len = if has_options
        && (handles.last() == Some(&check_opts_handle) || handles.last() == Some(&options_handle))
    {
        count.saturating_sub(1)
    } else {
        count
    };

    let pretty_opts = match pretty_mode {
        2 => PrettyPrintOptions::compact(),
        3 => PrettyPrintOptions::simple_color(),
        1 => PrettyPrintOptions::default(),
        _ => PrettyPrintOptions::no_color(),
    };

    // If file output is specified, write to file directly
    if let Some(ref path) = file_path {
        let mut out_str = String::new();
        for (i, &handle) in handles.iter().take(effective_len).enumerate() {
            if i > 0 {
                out_str.push_str(&sep);
            }
            let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
            if pretty_mode > 0 {
                out_str.push_str(&pretty_print(&val, &pretty_opts));
            } else {
                out_str.push_str(&val.as_string());
            }
        }
        out_str.push_str(&end);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(out_str.as_bytes());
        }
        return 0;
    }

    // Build ANSI codes
    let mut codes = Vec::new();
    if bold {
        codes.push("1".to_string());
    }
    if italic {
        codes.push("3".to_string());
    }
    if underline {
        codes.push("4".to_string());
    }
    if strikethrough {
        codes.push("9".to_string());
    }
    if let Some(ref hex) = color {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            codes.push(format!("38;2;{};{};{}", r, g, b));
        }
    }
    if let Some(ref hex) = background {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            codes.push(format!("48;2;{};{};{}", r, g, b));
        }
    }

    if !codes.is_empty() {
        print!("\x1b[{}m", codes.join(";"));
    }

    for (i, &handle) in handles.iter().take(effective_len).enumerate() {
        if i > 0 {
            print!("{}", sep);
        }
        let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
        if pretty_mode > 0 {
            let formatted = pretty_print(&val, &pretty_opts);
            print!("{}", formatted);
        } else {
            print!("{}", val.as_string());
        }
    }

    if !codes.is_empty() {
        print!("\x1b[0m");
    }

    if !end.is_empty() {
        print!("{}", end);
    }

    if flush || !end.is_empty() {
        let _ = std::io::stdout().flush();
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_free_handle(handle: u64) {
    aot_remove_value(handle);
}

// ============================================================================
// Sequence / Collection Operations
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_len(handle: u64) -> i64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => v.len() as i64,
        RuntimeValue::Tuple(v) => v.len() as i64,
        RuntimeValue::Set(v) => v.len() as i64,
        RuntimeValue::String(s) => s.len() as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_capacity(handle: u64) -> i64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => v.capacity() as i64,
        RuntimeValue::String(s) => s.capacity() as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_metadata_size(_handle: u64) -> i64 {
    24
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_first(handle: u64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => {
            if let Some(first) = v.first() {
                aot_store_value(first.clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_last(handle: u64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => {
            if let Some(last) = v.last() {
                aot_store_value(last.clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_get_index(container_handle: u64, index_handle: u64) -> u64 {
    let container =
        aot_get_value(container_handle).unwrap_or_else(|| unpack_aot_arg(container_handle));
    let idx_val = aot_get_value(index_handle).unwrap_or_else(|| unpack_aot_arg(index_handle));
    let index = idx_val.as_usize().unwrap_or(0);

    match container {
        RuntimeValue::Array(arr) => {
            if index < arr.len() {
                runtime_value_to_raw_or_handle(arr[index].clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        RuntimeValue::Tuple(tup) => {
            if index < tup.len() {
                runtime_value_to_raw_or_handle(tup[index].clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        RuntimeValue::String(s) => {
            if let Some(ch) = s.chars().nth(index) {
                runtime_value_to_raw_or_handle(RuntimeValue::Char(ch))
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        RuntimeValue::Object(obj) => {
            let key = idx_val.as_string();
            if let Some(val) = obj.get(&key) {
                runtime_value_to_raw_or_handle(val.clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_set_index(
    container_handle: u64,
    index_handle: u64,
    val_handle: u64,
) -> u64 {
    let mut container =
        aot_get_value(container_handle).unwrap_or_else(|| unpack_aot_arg(container_handle));
    let idx_val = aot_get_value(index_handle).unwrap_or_else(|| unpack_aot_arg(index_handle));
    let index = idx_val.as_usize().unwrap_or(0);
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));

    match container {
        RuntimeValue::Array(ref mut arr) => {
            if index < arr.len() {
                arr[index] = val;
            } else if index == arr.len() {
                arr.push(val);
            }
            aot_store_value(container)
        }
        RuntimeValue::Object(ref mut obj) => {
            let key = idx_val.as_string();
            obj.insert(key, val);
            aot_store_value(container)
        }
        _ => container_handle,
    }
}

// ============================================================================
// ARC / Reference Counting Runtime
// ============================================================================

struct ArcControlBlock {
    strong: AtomicI64,
    weak: AtomicI64,
    value: RuntimeValue,
}

static ARC_MAP: Mutex<Option<BTreeMap<u64, Arc<ArcControlBlock>>>> = Mutex::new(None);
static ARC_COUNTER: AtomicU64 = AtomicU64::new(1000);

fn get_arc_map() -> std::sync::MutexGuard<'static, Option<BTreeMap<u64, Arc<ArcControlBlock>>>> {
    let mut guard = ARC_MAP.lock().unwrap();
    if guard.is_none() {
        *guard = Some(BTreeMap::new());
    }
    guard
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_new(val_handle: u64) -> u64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    let block = Arc::new(ArcControlBlock {
        strong: AtomicI64::new(1),
        weak: AtomicI64::new(0),
        value: val,
    });
    let id = ARC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut map = get_arc_map();
    if let Some(ref mut m) = *map {
        m.insert(id, block);
    }
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_clone(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.strong.fetch_add(1, Ordering::SeqCst);
        }
    }
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_drop(handle: u64) -> u64 {
    let mut map = get_arc_map();
    if let Some(ref mut m) = *map {
        let should_remove = if let Some(block) = m.get(&handle) {
            let prev = block.strong.fetch_sub(1, Ordering::SeqCst);
            prev <= 1
        } else {
            false
        };
        if should_remove {
            m.remove(&handle);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_get(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return aot_store_value(block.value.clone());
        }
    }
    aot_store_value(RuntimeValue::Null)
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_set(_handle: u64, _val: u64) -> u64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_strong_count(handle: u64) -> i64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return block.strong.load(Ordering::SeqCst);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_weak_count(handle: u64) -> i64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return block.weak.load(Ordering::SeqCst);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_new(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.weak.fetch_add(1, Ordering::SeqCst);
        }
    }
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_drop(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.weak.fetch_sub(1, Ordering::SeqCst);
        }
    }
    0
}

// ============================================================================
// Memory Tracking / Heap Guard
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_assert_heap_allowed() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_alloc_tracked(size: i64, _metadata_ptr: i64) -> i64 {
    unsafe {
        let ptr = libc::malloc(size as usize);
        ptr as i64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_free_tracked(ptr: i64, _metadata_ptr: i64) -> i64 {
    if ptr != 0 {
        unsafe {
            libc::free(ptr as *mut libc::c_void);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_scope_exit(_scope_id: i64) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_validate_ptr(ptr: i64, _metadata_ptr: i64) -> i32 {
    if ptr != 0 { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_init_args(_argc: i32, _argv: *const *const c_char) {
    // Initialized command line args
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_value_to_string(value: u64) -> *mut c_char {
    let val = aot_get_value(value).unwrap_or_else(|| unpack_aot_arg(value));
    let s = val.as_string();
    let c_str = CString::new(s).unwrap_or_default();
    c_str.into_raw()
}

// ============================================================================
// Filesystem Support
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_read(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Null),
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => aot_store_value(RuntimeValue::String(s)),
        Err(_) => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_write(path_val: u64, content_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let content = match get_string_val(content_val) {
        Some(c) => c,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::write(&path, content) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_exists(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).exists()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_file(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).is_file()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_dir(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).is_dir()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_mkdir(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::create_dir_all(&path) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_delete(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let p = std::path::Path::new(&path);
    let res = if p.is_dir() {
        std::fs::remove_dir_all(&path).is_ok()
    } else {
        std::fs::remove_file(&path).is_ok()
    };
    aot_store_value(RuntimeValue::Bool(res))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_copy(src_val: u64, dst_val: u64) -> u64 {
    let src = match get_string_val(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Null),
    };
    let dst = match get_string_val(dst_val) {
        Some(d) => d,
        None => return aot_store_value(RuntimeValue::Null),
    };
    match std::fs::copy(&src, &dst) {
        Ok(n) => aot_store_value(RuntimeValue::Float(n as f64)),
        Err(_) => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_move(src_val: u64, dst_val: u64) -> u64 {
    let src = match get_string_val(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let dst = match get_string_val(dst_val) {
        Some(d) => d,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::rename(&src, &dst) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_fs_path_join(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::String(String::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut path = std::path::PathBuf::new();
    for &arg in args {
        if let Some(s) = get_string_val(arg) {
            path.push(s);
        }
    }
    aot_store_value(RuntimeValue::String(path.to_string_lossy().into_owned()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_basename(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let base = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(base))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_dirname(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let dir = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(dir))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_extname(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let ext = p
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(ext))
}

// ============================================================================
// Exception Handling Bridge
// ============================================================================

use std::cell::RefCell;

thread_local! {
    static CURRENT_EXCEPTION: RefCell<Option<RuntimeValue>> = const { RefCell::new(None) };
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_has_exception() -> i64 {
    CURRENT_EXCEPTION.with(|exc| if exc.borrow().is_some() { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_get_exception() -> u64 {
    CURRENT_EXCEPTION.with(|exc| {
        if let Some(val) = exc.borrow_mut().take() {
            aot_store_value(val)
        } else {
            aot_store_value(RuntimeValue::Null)
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_clear_exception() -> i64 {
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = None;
    });
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_throw_exception(val_handle: u64) -> i64 {
    let val = unpack_aot_arg(val_handle);
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = Some(val);
    });
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_typeof(handle: u64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    let type_name = val.type_name();
    aot_store_value(RuntimeValue::String(type_name.to_string()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_sizeof(handle: u64) -> i64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    val.size_of_value() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn sizeof(handle: u64) -> i64 {
    aot_sizeof(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn clock() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_array_for_each(arr_handle: u64, callback_fn: extern "C" fn(u64)) -> u64 {
    if arr_handle == 0 {
        return 0;
    }
    let val = aot_get_value(arr_handle).unwrap_or_else(|| unpack_aot_arg(arr_handle));
    if let RuntimeValue::Array(items) = val {
        for item in items {
            let item_h = aot_store_value(item);
            callback_fn(item_h);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_parallel_for_each(
    start: i64,
    end: i64,
    callback_fn: extern "C" fn(i64),
) -> i64 {
    for i in start..end {
        callback_fn(i);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_function(fn_ptr: usize) -> u64 {
    aot_store_value(RuntimeValue::Function(fn_ptr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_collections_new(
    type_name_handle: u64,
    args_ptr: *const u64,
    args_count: usize,
) -> u64 {
    let type_name = get_string_val(type_name_handle).unwrap_or_default();
    let raw_args = if args_ptr.is_null() || args_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(args_ptr, args_count)
    };
    let args: Vec<RuntimeValue> = raw_args
        .iter()
        .map(|&h| aot_get_value(h).unwrap_or_else(|| unpack_aot_arg(h)))
        .collect();

    match type_name.as_str() {
        "VecDeque" | "Queue" => {
            aot_store_value(RuntimeValue::VecDeque(std::collections::VecDeque::new()))
        }
        "HashSet" | "OrderedSet" => {
            aot_store_value(RuntimeValue::HashSet(std::collections::HashSet::new()))
        }
        "BTreeMap" | "OrderedMap" => {
            aot_store_value(RuntimeValue::BTreeMap(std::collections::BTreeMap::new()))
        }
        "HashMap" => aot_store_value(RuntimeValue::Object(std::collections::BTreeMap::new())),
        "BinaryHeap" => {
            aot_store_value(RuntimeValue::BinaryHeap(std::collections::BinaryHeap::new()))
        }
        "PriorityQueue" => aot_store_value(RuntimeValue::PriorityQueue(
            std::collections::BinaryHeap::new(),
        )),
        "BitSet" => aot_store_value(RuntimeValue::BitSet(vec![false; 64])),
        "RingBuffer" => {
            let cap = args.get(0).and_then(|v| v.as_usize()).unwrap_or(16);
            aot_store_value(RuntimeValue::RingBuffer {
                buffer: Vec::with_capacity(cap),
                head: 0,
                tail: 0,
                count: 0,
                cap,
            })
        }
        "Vec" | "Slice" | "Stack" => aot_store_value(RuntimeValue::Array(Vec::new())),
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_call_method(
    target_handle: u64,
    method_name_handle: u64,
    args_ptr: *const u64,
    args_count: usize,
) -> u64 {
    let method_name = get_string_val(method_name_handle).unwrap_or_default();
    let raw_args = if args_ptr.is_null() || args_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(args_ptr, args_count)
    };
    let args: Vec<RuntimeValue> = raw_args
        .iter()
        .map(|&h| aot_get_value(h).unwrap_or_else(|| unpack_aot_arg(h)))
        .collect();

    let target_val = aot_get_value(target_handle).unwrap_or_else(|| unpack_aot_arg(target_handle));

    match target_val {
        RuntimeValue::VecDeque(mut deque) => match method_name.as_str() {
            "pushBack" => {
                if let Some(arg0) = args.get(0) {
                    deque.push_back(arg0.clone());
                }
                aot_update_value(target_handle, RuntimeValue::VecDeque(deque));
                aot_store_value(RuntimeValue::Null)
            }
            "pushFront" => {
                if let Some(arg0) = args.get(0) {
                    deque.push_front(arg0.clone());
                }
                aot_update_value(target_handle, RuntimeValue::VecDeque(deque));
                aot_store_value(RuntimeValue::Null)
            }
            "popFront" => {
                let res = deque.pop_front().unwrap_or(RuntimeValue::Null);
                aot_update_value(target_handle, RuntimeValue::VecDeque(deque));
                aot_store_value(res)
            }
            "popBack" => {
                let res = deque.pop_back().unwrap_or(RuntimeValue::Null);
                aot_update_value(target_handle, RuntimeValue::VecDeque(deque));
                aot_store_value(res)
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(deque.len() as i64)),
            "isEmpty" => aot_store_value(RuntimeValue::Bool(deque.is_empty())),
            "clear" => {
                deque.clear();
                aot_update_value(target_handle, RuntimeValue::VecDeque(deque));
                aot_store_value(RuntimeValue::Null)
            }
            "get" => {
                let idx = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                let res = deque.get(idx).cloned().unwrap_or(RuntimeValue::Null);
                aot_store_value(res)
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::HashSet(mut set) => match method_name.as_str() {
            "insert" | "add" => {
                let inserted = if let Some(arg0) = args.get(0) {
                    set.insert(arg0.as_string())
                } else {
                    false
                };
                aot_update_value(target_handle, RuntimeValue::HashSet(set));
                aot_store_value(RuntimeValue::Bool(inserted))
            }
            "contains" | "has" => {
                let has = if let Some(arg0) = args.get(0) {
                    set.contains(&arg0.as_string())
                } else {
                    false
                };
                aot_store_value(RuntimeValue::Bool(has))
            }
            "remove" | "delete" => {
                let removed = if let Some(arg0) = args.get(0) {
                    set.remove(&arg0.as_string())
                } else {
                    false
                };
                aot_update_value(target_handle, RuntimeValue::HashSet(set));
                aot_store_value(RuntimeValue::Bool(removed))
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(set.len() as i64)),
            "clear" => {
                set.clear();
                aot_update_value(target_handle, RuntimeValue::HashSet(set));
                aot_store_value(RuntimeValue::Null)
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::BTreeMap(mut map) => match method_name.as_str() {
            "insert" | "set" => {
                if let (Some(k), Some(v)) = (args.get(0), args.get(1)) {
                    map.insert(k.as_string(), v.clone());
                }
                aot_update_value(target_handle, RuntimeValue::BTreeMap(map));
                aot_store_value(RuntimeValue::Null)
            }
            "get" => {
                let val = if let Some(k) = args.get(0) {
                    map.get(&k.as_string())
                        .cloned()
                        .unwrap_or(RuntimeValue::Null)
                } else {
                    RuntimeValue::Null
                };
                aot_store_value(val)
            }
            "remove" | "delete" => {
                let val = if let Some(k) = args.get(0) {
                    map.remove(&k.as_string()).unwrap_or(RuntimeValue::Null)
                } else {
                    RuntimeValue::Null
                };
                aot_update_value(target_handle, RuntimeValue::BTreeMap(map));
                aot_store_value(val)
            }
            "contains" | "containsKey" | "has" => {
                let has = if let Some(k) = args.get(0) {
                    map.contains_key(&k.as_string())
                } else {
                    false
                };
                aot_store_value(RuntimeValue::Bool(has))
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(map.len() as i64)),
            "keys" => {
                let keys = map
                    .keys()
                    .map(|k| RuntimeValue::String(k.clone()))
                    .collect();
                aot_store_value(RuntimeValue::Array(keys))
            }
            "values" => {
                let vals = map.values().cloned().collect();
                aot_store_value(RuntimeValue::Array(vals))
            }
            "clear" => {
                map.clear();
                aot_update_value(target_handle, RuntimeValue::BTreeMap(map));
                aot_store_value(RuntimeValue::Null)
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::BinaryHeap(mut heap) => match method_name.as_str() {
            "push" | "insert" => {
                if let Some(arg0) = args.get(0) {
                    heap.push(OrderedValue(arg0.clone()));
                }
                aot_update_value(target_handle, RuntimeValue::BinaryHeap(heap));
                aot_store_value(RuntimeValue::Null)
            }
            "pop" => {
                let res = heap.pop().map(|o| o.0).unwrap_or(RuntimeValue::Null);
                aot_update_value(target_handle, RuntimeValue::BinaryHeap(heap));
                aot_store_value(res)
            }
            "peek" => {
                let res = heap
                    .peek()
                    .map(|o| o.0.clone())
                    .unwrap_or(RuntimeValue::Null);
                aot_store_value(res)
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(heap.len() as i64)),
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::PriorityQueue(mut pq) => match method_name.as_str() {
            "push" => {
                if let Some(item) = args.get(0) {
                    let prio = args.get(1).and_then(|p| p.as_i64()).unwrap_or(0);
                    pq.push(PriorityItem {
                        item: item.clone(),
                        priority: prio,
                    });
                }
                aot_update_value(target_handle, RuntimeValue::PriorityQueue(pq));
                aot_store_value(RuntimeValue::Null)
            }
            "pop" => {
                let res = pq.pop().map(|p| p.item).unwrap_or(RuntimeValue::Null);
                aot_update_value(target_handle, RuntimeValue::PriorityQueue(pq));
                aot_store_value(res)
            }
            "peek" => {
                let res = pq
                    .peek()
                    .map(|p| p.item.clone())
                    .unwrap_or(RuntimeValue::Null);
                aot_store_value(res)
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(pq.len() as i64)),
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::BitSet(mut bits) => match method_name.as_str() {
            "set" => {
                let idx = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                if idx >= bits.len() {
                    bits.resize(idx + 64, false);
                }
                bits[idx] = true;
                aot_update_value(target_handle, RuntimeValue::BitSet(bits));
                aot_store_value(RuntimeValue::Null)
            }
            "clear" => {
                if let Some(idx_v) = args.get(0) {
                    let idx = idx_v.as_usize().unwrap_or(0);
                    if idx < bits.len() {
                        bits[idx] = false;
                    }
                } else {
                    bits.clear();
                }
                aot_update_value(target_handle, RuntimeValue::BitSet(bits));
                aot_store_value(RuntimeValue::Null)
            }
            "test" | "get" => {
                let idx = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                let b = bits.get(idx).copied().unwrap_or(false);
                aot_store_value(RuntimeValue::Bool(b))
            }
            "toggle" => {
                let idx = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                if idx >= bits.len() {
                    bits.resize(idx + 64, false);
                }
                bits[idx] = !bits[idx];
                aot_update_value(target_handle, RuntimeValue::BitSet(bits));
                aot_store_value(RuntimeValue::Null)
            }
            "count" => {
                let cnt = bits.iter().filter(|&&b| b).count() as i64;
                aot_store_value(RuntimeValue::Int(cnt))
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::RingBuffer {
            mut buffer,
            mut head,
            mut tail,
            mut count,
            cap,
        } => match method_name.as_str() {
            "push" => {
                if let Some(arg0) = args.get(0) {
                    if count < cap {
                        if buffer.len() < cap {
                            buffer.push(arg0.clone());
                        } else {
                            buffer[tail] = arg0.clone();
                        }
                        tail = (tail + 1) % cap;
                        count += 1;
                    } else {
                        buffer[tail] = arg0.clone();
                        tail = (tail + 1) % cap;
                        head = (head + 1) % cap;
                    }
                }
                aot_update_value(
                    target_handle,
                    RuntimeValue::RingBuffer {
                        buffer,
                        head,
                        tail,
                        count,
                        cap,
                    },
                );
                aot_store_value(RuntimeValue::Null)
            }
            "pop" => {
                let res = if count == 0 {
                    RuntimeValue::Null
                } else {
                    let val = buffer[head].clone();
                    head = (head + 1) % cap;
                    count -= 1;
                    val
                };
                aot_update_value(
                    target_handle,
                    RuntimeValue::RingBuffer {
                        buffer,
                        head,
                        tail,
                        count,
                        cap,
                    },
                );
                aot_store_value(res)
            }
            "peek" => {
                let res = if count == 0 {
                    RuntimeValue::Null
                } else {
                    buffer[head].clone()
                };
                aot_store_value(res)
            }
            "isFull" => aot_store_value(RuntimeValue::Bool(count == cap)),
            "isEmpty" => aot_store_value(RuntimeValue::Bool(count == 0)),
            "capacity" => aot_store_value(RuntimeValue::Int(cap as i64)),
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::Array(mut arr) => match method_name.as_str() {
            "append" | "push" => {
                if let Some(arg0) = args.get(0) {
                    arr.push(arg0.clone());
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "extend" => {
                if let Some(RuntimeValue::Array(other)) = args.get(0) {
                    arr.extend(other.clone());
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "insert" => {
                if let (Some(idx_val), Some(item)) = (args.get(0), args.get(1)) {
                    let idx = idx_val.as_usize().unwrap_or(0).min(arr.len());
                    arr.insert(idx, item.clone());
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "remove" => {
                if let Some(item_val) = args.get(0) {
                    if let Some(pos) = arr.iter().position(|x| x == item_val) {
                        arr.remove(pos);
                    }
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "set_index" => {
                if let (Some(idx_val), Some(item)) = (args.get(0), args.get(1)) {
                    let idx = idx_val.as_usize().unwrap_or(0);
                    if idx < arr.len() {
                        arr[idx] = item.clone();
                    }
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "pop" => {
                if let Some(idx_val) = args.get(0) {
                    let idx = idx_val.as_usize().unwrap_or(0);
                    if idx < arr.len() {
                        arr.remove(idx);
                    }
                } else {
                    arr.pop();
                }
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "count" => {
                let target_item = args.get(0);
                let cnt = if let Some(t) = target_item {
                    arr.iter().filter(|x| *x == t).count()
                } else {
                    0
                };
                aot_store_value(RuntimeValue::Int(cnt as i64))
            }
            "index" => {
                let target_item = args.get(0);
                let pos = if let Some(t) = target_item {
                    arr.iter()
                        .position(|x| x == t)
                        .map(|i| i as i64)
                        .unwrap_or(-1)
                } else {
                    -1
                };
                aot_store_value(RuntimeValue::Int(pos))
            }
            "clear" => {
                arr.clear();
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "sort" => {
                arr.sort_by(|a, b| OrderedValue(a.clone()).cmp(&OrderedValue(b.clone())));
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "reverse" => {
                arr.reverse();
                aot_update_value(target_handle, RuntimeValue::Array(arr.clone()));
                aot_store_value(RuntimeValue::Array(arr))
            }
            "map" => {
                if let Some(fn_val) = args.get(0) {
                    if let Some(fp) = fn_val.as_fn_ptr() {
                        let f = std::mem::transmute::<usize, extern "C" fn(i64) -> u64>(fp);
                        let mut new_arr = Vec::with_capacity(arr.len());
                        for item in &arr {
                            let in_h = aot_store_value(item.clone());
                            let arg_val = item.as_i64().unwrap_or(in_h as i64);
                            let out_h = f(arg_val);
                            let out_val =
                                aot_get_value(out_h).unwrap_or_else(|| unpack_aot_arg(out_h));
                            new_arr.push(out_val);
                        }
                        return aot_store_value(RuntimeValue::Array(new_arr));
                    }
                }
                aot_store_value(RuntimeValue::Array(arr))
            }
            "filter" => {
                if let Some(fn_val) = args.get(0) {
                    if let Some(fp) = fn_val.as_fn_ptr() {
                        let f = std::mem::transmute::<usize, extern "C" fn(i64) -> u64>(fp);
                        let mut new_arr = Vec::new();
                        for item in &arr {
                            let in_h = aot_store_value(item.clone());
                            let arg_val = item.as_i64().unwrap_or(in_h as i64);
                            let out_h = f(arg_val);
                            let out_val =
                                aot_get_value(out_h).unwrap_or_else(|| unpack_aot_arg(out_h));
                            if out_val.is_truthy() {
                                new_arr.push(item.clone());
                            }
                        }
                        return aot_store_value(RuntimeValue::Array(new_arr));
                    }
                }
                aot_store_value(RuntimeValue::Array(arr))
            }
            "reduce" => {
                if let Some(fn_val) = args.get(0) {
                    if let Some(fp) = fn_val.as_fn_ptr() {
                        let f = std::mem::transmute::<usize, extern "C" fn(i64, i64) -> u64>(fp);
                        let has_init = args.len() > 1;
                        let mut acc = if has_init {
                            args[1].clone()
                        } else if !arr.is_empty() {
                            arr[0].clone()
                        } else {
                            RuntimeValue::Null
                        };
                        let start_idx = if has_init { 0 } else { 1 };
                        for item in arr.iter().skip(start_idx) {
                            let acc_h = aot_store_value(acc.clone());
                            let item_h = aot_store_value(item.clone());
                            let acc_arg = acc.as_i64().unwrap_or(acc_h as i64);
                            let item_arg = item.as_i64().unwrap_or(item_h as i64);
                            let out_h = f(acc_arg, item_arg);
                            acc = aot_get_value(out_h).unwrap_or_else(|| unpack_aot_arg(out_h));
                        }
                        return aot_store_value(acc);
                    }
                }
                aot_store_value(RuntimeValue::Null)
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(arr.len() as i64)),
            "join" => {
                let sep = args.get(0).map(|s| s.as_string()).unwrap_or_default();
                let str_items: Vec<String> = arr.iter().map(|x| x.as_string()).collect();
                aot_store_value(RuntimeValue::String(str_items.join(&sep)))
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::Set(set) => match method_name.as_str() {
            "union" => {
                let mut union_items = set.clone();
                if let Some(RuntimeValue::Set(other)) = args.get(0) {
                    for item in other {
                        if !union_items.contains(item) {
                            union_items.push(item.clone());
                        }
                    }
                }
                aot_store_value(RuntimeValue::Set(union_items))
            }
            "intersection" => {
                let mut inter_items = Vec::new();
                if let Some(RuntimeValue::Set(other)) = args.get(0) {
                    for item in &set {
                        if other.contains(item) {
                            inter_items.push(item.clone());
                        }
                    }
                }
                aot_store_value(RuntimeValue::Set(inter_items))
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(set.len() as i64)),
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::Object(mut map) => match method_name.as_str() {
            "get" => {
                let val = if let Some(k) = args.get(0) {
                    map.get(&k.as_string())
                        .cloned()
                        .unwrap_or(RuntimeValue::Null)
                } else {
                    RuntimeValue::Null
                };
                aot_store_value(val)
            }
            "set" => {
                if let (Some(k), Some(v)) = (args.get(0), args.get(1)) {
                    map.insert(k.as_string(), v.clone());
                }
                aot_update_value(target_handle, RuntimeValue::Object(map.clone()));
                aot_store_value(RuntimeValue::Object(map))
            }
            "has" | "hasOwnProperty" => {
                let has = if let Some(k) = args.get(0) {
                    map.contains_key(&k.as_string())
                } else {
                    false
                };
                aot_store_value(RuntimeValue::Bool(has))
            }
            "keys" => {
                let keys = map
                    .keys()
                    .map(|k| RuntimeValue::String(k.clone()))
                    .collect();
                aot_store_value(RuntimeValue::Array(keys))
            }
            "values" => {
                let vals = map.values().cloned().collect();
                aot_store_value(RuntimeValue::Array(vals))
            }
            "entries" => {
                let entries = map
                    .iter()
                    .map(|(k, v)| {
                        RuntimeValue::Tuple(vec![RuntimeValue::String(k.clone()), v.clone()])
                    })
                    .collect();
                aot_store_value(RuntimeValue::Array(entries))
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(map.len() as i64)),
            _ => aot_store_value(RuntimeValue::Null),
        },
        RuntimeValue::String(s) => match method_name.as_str() {
            "toLowerCase" => aot_store_value(RuntimeValue::String(s.to_lowercase())),
            "toUpperCase" => aot_store_value(RuntimeValue::String(s.to_uppercase())),
            "trim" => aot_store_value(RuntimeValue::String(s.trim().to_string())),
            "split" => {
                let sep = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                let parts = s
                    .split(&sep)
                    .map(|p| RuntimeValue::String(p.to_string()))
                    .collect();
                aot_store_value(RuntimeValue::Array(parts))
            }
            "replace" => {
                let from = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                let to = args.get(1).map(|x| x.as_string()).unwrap_or_default();
                aot_store_value(RuntimeValue::String(s.replace(&from, &to)))
            }
            "substring" | "slice" => {
                let start = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                let end = if args.len() > 1 {
                    args[1].as_usize().unwrap_or(s.len())
                } else {
                    s.len()
                };
                let sub = if start < s.len() {
                    let end = end.min(s.len());
                    &s[start..end]
                } else {
                    ""
                };
                aot_store_value(RuntimeValue::String(sub.to_string()))
            }
            "contains" | "includes" => {
                let sub = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                aot_store_value(RuntimeValue::Bool(s.contains(&sub)))
            }
            "indexOf" => {
                let sub = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                let pos = s.find(&sub).map(|i| i as i64).unwrap_or(-1);
                aot_store_value(RuntimeValue::Int(pos))
            }
            "startsWith" => {
                let prefix = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                aot_store_value(RuntimeValue::Bool(s.starts_with(&prefix)))
            }
            "endsWith" => {
                let suffix = args.get(0).map(|x| x.as_string()).unwrap_or_default();
                aot_store_value(RuntimeValue::Bool(s.ends_with(&suffix)))
            }
            "len" | "length" => aot_store_value(RuntimeValue::Int(s.len() as i64)),
            "charAt" => {
                let idx = args.get(0).and_then(|i| i.as_usize()).unwrap_or(0);
                let ch = s
                    .chars()
                    .nth(idx)
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                aot_store_value(RuntimeValue::String(ch))
            }
            _ => aot_store_value(RuntimeValue::Null),
        },
        _ => aot_store_value(RuntimeValue::Null),
    }
}

// ============================================================================
// Input & Type Conversions
// ============================================================================

static MOCK_INPUT_QUEUE: Mutex<Option<std::collections::VecDeque<String>>> = Mutex::new(None);

fn get_mock_input_queue()
-> std::sync::MutexGuard<'static, Option<std::collections::VecDeque<String>>> {
    let mut guard = MOCK_INPUT_QUEUE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(std::collections::VecDeque::new());
    }
    guard
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_mock(val_handle: u64) -> u64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    let mut guard = get_mock_input_queue();
    if let Some(ref mut q) = *guard {
        if let RuntimeValue::Array(arr) = val {
            for item in arr {
                q.push_back(item.as_string());
            }
        } else {
            q.push_back(val.as_string());
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input(prompt_handle: u64) -> u64 {
    let prompt = if prompt_handle != 0 {
        get_string_val(prompt_handle).unwrap_or_default()
    } else {
        String::new()
    };
    if !prompt.is_empty() {
        print!("{}", prompt);
        let _ = std::io::stdout().flush();
    }
    let mut guard = get_mock_input_queue();
    if let Some(ref mut q) = *guard {
        if let Some(mock_val) = q.pop_front() {
            return aot_store_value(RuntimeValue::String(mock_val));
        }
    }
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    aot_store_value(RuntimeValue::String(line))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_to_int(val_handle: u64) -> i64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    match val {
        RuntimeValue::Int(i) | RuntimeValue::I64(i) => i,
        RuntimeValue::I32(i) => i as i64,
        RuntimeValue::I16(i) => i as i64,
        RuntimeValue::I8(i) => i as i64,
        RuntimeValue::U64(u) => u as i64,
        RuntimeValue::U32(u) => u as i64,
        RuntimeValue::U16(u) => u as i64,
        RuntimeValue::U8(u) => u as i64,
        RuntimeValue::Float(f) | RuntimeValue::F64(f) => f as i64,
        RuntimeValue::F32(f) => f as i64,
        RuntimeValue::Bool(b) => {
            if b {
                1
            } else {
                0
            }
        }
        RuntimeValue::String(s) => s
            .trim()
            .parse::<i64>()
            .unwrap_or_else(|_| s.trim().parse::<f64>().map(|f| f as i64).unwrap_or(0)),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_to_float(val_handle: u64) -> f64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    match val {
        RuntimeValue::Float(f) | RuntimeValue::F64(f) => f,
        RuntimeValue::F32(f) => f as f64,
        RuntimeValue::Int(i) | RuntimeValue::I64(i) => i as f64,
        RuntimeValue::I32(i) => i as f64,
        RuntimeValue::I16(i) => i as f64,
        RuntimeValue::I8(i) => i as f64,
        RuntimeValue::U64(u) => u as f64,
        RuntimeValue::U32(u) => u as f64,
        RuntimeValue::U16(u) => u as f64,
        RuntimeValue::U8(u) => u as f64,
        RuntimeValue::Bool(b) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
        RuntimeValue::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_to_string(val_handle: u64) -> u64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    aot_store_value(RuntimeValue::String(val.as_string()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_to_bool(val_handle: u64) -> i64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    if val.is_truthy() { 1 } else { 0 }
}
