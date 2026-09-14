//! Runtime function declarations and string data management

use cranelift::prelude::*;
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use super::context::FunctionCompileContext;
use super::utilities::add_format_string;

/// Get the appropriate calling convention for the target platform
#[allow(dead_code)]
fn get_calling_convention(triple: &Triple) -> Result<isa::CallConv, String> {
    match triple.operating_system {
        target_lexicon::OperatingSystem::Windows => {
            // Windows uses fastcall convention
            Ok(isa::CallConv::WindowsFastcall)
        }
        target_lexicon::OperatingSystem::MacOSX { .. }
        | target_lexicon::OperatingSystem::Darwin => {
            // macOS uses System V
            Ok(isa::CallConv::SystemV)
        }
        target_lexicon::OperatingSystem::Linux => {
            // Linux and Android use System V
            Ok(isa::CallConv::SystemV)
        }
        _ => {
            // Default to System V for unknown platforms
            Ok(isa::CallConv::SystemV)
        }
    }
}

/// Declare runtime functions that the compiled code will call
#[allow(dead_code)]
pub(super) fn declare_runtime_functions(
    module: &mut ObjectModule,
    ctx: &mut FunctionCompileContext,
    target_triple: &Triple,
) -> Result<(), String> {
    // Determine calling convention based on target platform
    let call_conv = get_calling_convention(target_triple)?;

    // Declare printf - takes format string pointer, returns int
    // printf(const char* fmt, ...) -> int
    // For simplicity, we'll use a fixed signature with one i64 arg
    let mut printf_sig = Signature::new(call_conv);
    printf_sig.params.push(AbiParam::new(types::I64)); // format string pointer
    printf_sig.params.push(AbiParam::new(types::I64)); // first argument (for %lld)
    printf_sig.returns.push(AbiParam::new(types::I32)); // return value

    let printf_id = module
        .declare_function("printf", Linkage::Import, &printf_sig)
        .map_err(|e| format!("Failed to declare printf: {}", e))?;
    ctx.printf_func = Some(printf_id);

    // Declare puts - takes string pointer, returns int
    let mut puts_sig = Signature::new(call_conv);
    puts_sig.params.push(AbiParam::new(types::I64)); // string pointer
    puts_sig.returns.push(AbiParam::new(types::I32)); // return value

    let puts_id = module
        .declare_function("puts", Linkage::Import, &puts_sig)
        .map_err(|e| format!("Failed to declare puts: {}", e))?;
    ctx.puts_func = Some(puts_id);

    // Declare malloc - takes size (i64), returns pointer (i64)
    // void* malloc(size_t size)
    let mut malloc_sig = Signature::new(call_conv);
    malloc_sig.params.push(AbiParam::new(types::I64)); // size
    malloc_sig.returns.push(AbiParam::new(types::I64)); // pointer

    let malloc_id = module
        .declare_function("malloc", Linkage::Import, &malloc_sig)
        .map_err(|e| format!("Failed to declare malloc: {}", e))?;
    ctx.malloc_func = Some(malloc_id);

    // Declare memcpy - takes dest ptr, src ptr, size, returns dest ptr
    // void* memcpy(void* dest, const void* src, size_t n)
    let mut memcpy_sig = Signature::new(call_conv);
    memcpy_sig.params.push(AbiParam::new(types::I64)); // dest
    memcpy_sig.params.push(AbiParam::new(types::I64)); // src
    memcpy_sig.params.push(AbiParam::new(types::I64)); // size
    memcpy_sig.returns.push(AbiParam::new(types::I64)); // pointer

    let memcpy_id = module
        .declare_function("memcpy", Linkage::Import, &memcpy_sig)
        .map_err(|e| format!("Failed to declare memcpy: {}", e))?;
    ctx.memcpy_func = Some(memcpy_id);

    // Declare free - takes pointer (i64), returns void (we use i64 dummy)
    // void free(void* ptr)
    let mut free_sig = Signature::new(call_conv);
    free_sig.params.push(AbiParam::new(types::I64)); // ptr
    // Cranelift requires at least one return in our pipeline; use I64 zero if needed
    free_sig.returns.push(AbiParam::new(types::I64));
    let free_id = module
        .declare_function("free", Linkage::Import, &free_sig)
        .map_err(|e| format!("Failed to declare free: {}", e))?;
    ctx.free_func = Some(free_id);

    // Declare adesh_rt_assert_heap_allowed - returns int (0/1) or aborts
    let mut heap_guard_sig = Signature::new(call_conv);
    heap_guard_sig.returns.push(AbiParam::new(types::I32));
    if let Ok(heap_guard_id) = module.declare_function(
        "adesh_rt_assert_heap_allowed",
        Linkage::Import,
        &heap_guard_sig,
    ) {
        ctx.heap_guard_func = Some(heap_guard_id);
    }

    // ARC/Weak runtime functions
    let mut arc_new_sig = Signature::new(call_conv);
    arc_new_sig.params.push(AbiParam::new(types::I64)); // value ptr or value
    arc_new_sig.returns.push(AbiParam::new(types::I64)); // arc handle
    let _ = module.declare_function("adesh_rt_arc_new", Linkage::Import, &arc_new_sig);

    let mut arc_clone_sig = Signature::new(call_conv);
    arc_clone_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_clone_sig.returns.push(AbiParam::new(types::I64)); // new arc handle
    let _ = module.declare_function("adesh_rt_arc_clone", Linkage::Import, &arc_clone_sig);

    let mut arc_drop_sig = Signature::new(call_conv);
    arc_drop_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_drop_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("adesh_rt_arc_drop", Linkage::Import, &arc_drop_sig);

    let mut weak_new_sig = Signature::new(call_conv);
    weak_new_sig.params.push(AbiParam::new(types::I64)); // arc handle
    weak_new_sig.returns.push(AbiParam::new(types::I64)); // weak handle
    let _ = module.declare_function("adesh_rt_weak_new", Linkage::Import, &weak_new_sig);

    let mut weak_drop_sig = Signature::new(call_conv);
    weak_drop_sig.params.push(AbiParam::new(types::I64)); // weak handle
    weak_drop_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("adesh_rt_weak_drop", Linkage::Import, &weak_drop_sig);

    let mut arc_get_sig = Signature::new(call_conv);
    arc_get_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_get_sig.returns.push(AbiParam::new(types::I64)); // value ptr or value
    let _ = module.declare_function("adesh_rt_arc_get", Linkage::Import, &arc_get_sig);

    let mut arc_set_sig = Signature::new(call_conv);
    arc_set_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_set_sig.params.push(AbiParam::new(types::I64)); // value
    arc_set_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("adesh_rt_arc_set", Linkage::Import, &arc_set_sig);

    let mut arc_sc_sig = Signature::new(call_conv);
    arc_sc_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_sc_sig.returns.push(AbiParam::new(types::I64)); // strong count
    let _ = module.declare_function("adesh_rt_arc_strong_count", Linkage::Import, &arc_sc_sig);

    let mut arc_wc_sig = Signature::new(call_conv);
    arc_wc_sig.params.push(AbiParam::new(types::I64)); // arc handle
    arc_wc_sig.returns.push(AbiParam::new(types::I64)); // weak count
    let _ = module.declare_function("adesh_rt_arc_weak_count", Linkage::Import, &arc_wc_sig);

    // Create a global array buffer (64KB should be enough for most cases)
    let buffer_size = 65536;
    let mut buffer_data = DataDescription::new();
    buffer_data.define_zeroinit(buffer_size);
    buffer_data.set_align(16); // 16-byte alignment for safety

    let buffer_id = module
        .declare_data("__array_buffer", Linkage::Local, true, false)
        .map_err(|e| format!("Failed to declare array buffer: {}", e))?;
    module
        .define_data(buffer_id, &buffer_data)
        .map_err(|e| format!("Failed to define array buffer: {}", e))?;
    ctx.array_buffer = Some(buffer_id);

    // Create a global offset counter (starts at 0)
    let mut offset_data = DataDescription::new();
    offset_data.define(vec![0u8; 8].into_boxed_slice()); // 8 bytes for i64 offset
    offset_data.set_align(8);

    let offset_id = module
        .declare_data("__array_offset", Linkage::Local, true, false)
        .map_err(|e| format!("Failed to declare array offset: {}", e))?;
    module
        .define_data(offset_id, &offset_data)
        .map_err(|e| format!("Failed to define array offset: {}", e))?;
    ctx.array_offset = Some(offset_id);

    // Create global variables for command-line arguments
    let mut argc_data = DataDescription::new();
    argc_data.define(vec![0u8; 4].into_boxed_slice()); // 4 bytes for i32 argc
    let argc_id = module
        .declare_data("__argc", Linkage::Local, true, false)
        .map_err(|e| format!("Failed to declare argc global: {}", e))?;
    module
        .define_data(argc_id, &argc_data)
        .map_err(|e| format!("Failed to define argc global: {}", e))?;
    ctx.argc_global = Some(argc_id);

    let mut argv_data = DataDescription::new();
    argv_data.define(vec![0u8; 8].into_boxed_slice()); // 8 bytes for i64 argv pointer
    let argv_id = module
        .declare_data("__argv", Linkage::Local, true, false)
        .map_err(|e| format!("Failed to declare argv global: {}", e))?;
    module
        .define_data(argv_id, &argv_data)
        .map_err(|e| format!("Failed to define argv global: {}", e))?;
    ctx.argv_global = Some(argv_id);

    // Declare AOT runtime functions for unified pretty printing

    // aot_make_object - create object from key-value pairs
    let mut make_obj_sig = Signature::new(call_conv);
    make_obj_sig.params.push(AbiParam::new(types::I64)); // args_ptr
    make_obj_sig.params.push(AbiParam::new(types::I64)); // arg_count
    make_obj_sig.returns.push(AbiParam::new(types::I64)); // handle
    let _ = module.declare_function("aot_make_object", Linkage::Import, &make_obj_sig);

    // aot_print_value_pretty - pretty print a value by handle
    let mut print_pretty_sig = Signature::new(call_conv);
    print_pretty_sig.params.push(AbiParam::new(types::I64)); // handle
    print_pretty_sig.params.push(AbiParam::new(types::I64)); // mode_ptr
    print_pretty_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_value_pretty", Linkage::Import, &print_pretty_sig);

    // aot_print_newline - print newline
    let mut print_nl_sig = Signature::new(call_conv);
    print_nl_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_newline", Linkage::Import, &print_nl_sig);

    // aot_print_space - print space
    let mut print_space_sig = Signature::new(call_conv);
    print_space_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_space", Linkage::Import, &print_space_sig);

    // aot_print_i64 - print integer
    let mut print_i64_sig = Signature::new(call_conv);
    print_i64_sig.params.push(AbiParam::new(types::I64)); // value
    print_i64_sig.params.push(AbiParam::new(types::I64)); // newline flag
    print_i64_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_i64", Linkage::Import, &print_i64_sig);

    // aot_print_f64 - print float
    let mut print_f64_sig = Signature::new(call_conv);
    print_f64_sig.params.push(AbiParam::new(types::F64)); // value
    print_f64_sig.params.push(AbiParam::new(types::I64)); // newline flag
    print_f64_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_f64", Linkage::Import, &print_f64_sig);

    // aot_print_str - print string
    let mut print_str_sig = Signature::new(call_conv);
    print_str_sig.params.push(AbiParam::new(types::I64)); // string_ptr
    print_str_sig.params.push(AbiParam::new(types::I64)); // newline flag
    print_str_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_str", Linkage::Import, &print_str_sig);

    // aot_free_handle - free runtime value
    let mut free_handle_sig = Signature::new(call_conv);
    free_handle_sig.params.push(AbiParam::new(types::I64)); // handle
    free_handle_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_free_handle", Linkage::Import, &free_handle_sig);

    // aot_print_with_options - print values with options object
    let mut print_opts_sig = Signature::new(call_conv);
    print_opts_sig.params.push(AbiParam::new(types::I64)); // values_ptr (array of handles)
    print_opts_sig.params.push(AbiParam::new(types::I64)); // values_count
    print_opts_sig.params.push(AbiParam::new(types::I64)); // options_handle (or 0 for none)
    print_opts_sig.returns.push(AbiParam::new(types::I64)); // dummy
    let _ = module.declare_function("aot_print_with_options", Linkage::Import, &print_opts_sig);

    // Declare filesystem runtime functions
    let mut fs_unary_sig = Signature::new(call_conv);
    fs_unary_sig.params.push(AbiParam::new(types::I64)); // path_val (handle/ptr)
    fs_unary_sig.returns.push(AbiParam::new(types::I64)); // handle
    let _ = module.declare_function("aot_fs_read", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_exists", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_is_file", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_is_dir", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_mkdir", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_delete", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_path_basename", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_path_dirname", Linkage::Import, &fs_unary_sig);
    let _ = module.declare_function("aot_fs_path_extname", Linkage::Import, &fs_unary_sig);

    let mut fs_binary_sig = Signature::new(call_conv);
    fs_binary_sig.params.push(AbiParam::new(types::I64));
    fs_binary_sig.params.push(AbiParam::new(types::I64));
    fs_binary_sig.returns.push(AbiParam::new(types::I64));
    let _ = module.declare_function("aot_fs_write", Linkage::Import, &fs_binary_sig);
    let _ = module.declare_function("aot_fs_copy", Linkage::Import, &fs_binary_sig);
    let _ = module.declare_function("aot_fs_move", Linkage::Import, &fs_binary_sig);

    let mut fs_path_join_sig = Signature::new(call_conv);
    fs_path_join_sig.params.push(AbiParam::new(types::I64)); // args_ptr
    fs_path_join_sig.params.push(AbiParam::new(types::I64)); // arg_count
    fs_path_join_sig.returns.push(AbiParam::new(types::I64));
    let _ = module.declare_function("aot_fs_path_join", Linkage::Import, &fs_path_join_sig);

    Ok(())
}

/// Create string data in the data section
#[allow(dead_code)]
pub(super) fn create_string_data(
    module: &mut ObjectModule,
    ctx: &mut FunctionCompileContext,
    strings: &[String],
    _target_triple: &Triple,
) -> Result<(), String> {
    for s in strings {
        if ctx.string_data.contains_key(s) {
            continue;
        }

        // Create null-terminated string data
        // Filter out null bytes from the string to avoid object writer panic
        let sanitized: Vec<u8> = s.as_bytes().iter().filter(|&&b| b != 0).copied().collect();
        let mut data = sanitized;
        data.push(0); // null terminator

        // Generate a sanitized name for the data section (no null bytes)
        let data_name = format!("str_{}", ctx.string_data.len());

        let data_id = module
            .declare_data(
                &data_name,
                Linkage::Local,
                false, // not writable
                false, // not TLS
            )
            .map_err(|e| format!("Failed to declare string data: {}", e))?;

        let mut desc = DataDescription::new();
        desc.define(data.into_boxed_slice());
        module
            .define_data(data_id, &desc)
            .map_err(|e| format!("Failed to define string data: {}", e))?;

        ctx.string_data.insert(s.clone(), data_id);
    }

    // Also create format strings for printf
    // "%lld\n" for integer printing
    let int_fmt = "%lld\n";
    if !ctx.string_data.contains_key(int_fmt) {
        let mut data = int_fmt.as_bytes().to_vec();
        data.push(0);
        let data_id = module
            .declare_data("fmt_int", Linkage::Local, false, false)
            .map_err(|e| format!("Failed to declare format string: {}", e))?;
        let mut desc = DataDescription::new();
        desc.define(data.into_boxed_slice());
        module
            .define_data(data_id, &desc)
            .map_err(|e| format!("Failed to define format string: {}", e))?;
        ctx.string_data.insert(int_fmt.to_string(), data_id);
    }

    // "%g\n" for float printing
    let float_fmt = "%g\n";
    if !ctx.string_data.contains_key(float_fmt) {
        let mut data = float_fmt.as_bytes().to_vec();
        data.push(0);
        let data_id = module
            .declare_data("fmt_float", Linkage::Local, false, false)
            .map_err(|e| format!("Failed to declare format string: {}", e))?;
        let mut desc = DataDescription::new();
        desc.define(data.into_boxed_slice());
        module
            .define_data(data_id, &desc)
            .map_err(|e| format!("Failed to define format string: {}", e))?;
        ctx.string_data.insert(float_fmt.to_string(), data_id);
    }

    // "%s\n" for string printing
    let str_fmt = "%s\n";
    if !ctx.string_data.contains_key(str_fmt) {
        let mut data = str_fmt.as_bytes().to_vec();
        data.push(0);
        let data_id = module
            .declare_data("fmt_str", Linkage::Local, false, false)
            .map_err(|e| format!("Failed to declare format string: {}", e))?;
        let mut desc = DataDescription::new();
        desc.define(data.into_boxed_slice());
        module
            .define_data(data_id, &desc)
            .map_err(|e| format!("Failed to define format string: {}", e))?;
        ctx.string_data.insert(str_fmt.to_string(), data_id);
    }

    // Additional format strings for print features
    // "%s" for string without newline
    add_format_string(module, ctx, "%s", "fmt_str_no_nl")?;
    // "%lld" for integer without newline
    add_format_string(module, ctx, "%lld", "fmt_int_no_nl")?;
    // "%g" for float without newline
    add_format_string(module, ctx, "%g", "fmt_float_no_nl")?;
    // " " space separator
    add_format_string(module, ctx, " ", "fmt_space")?;
    // ", " comma separator
    add_format_string(module, ctx, ", ", "fmt_comma")?;
    // "\n" newline
    add_format_string(module, ctx, "\n", "fmt_newline")?;
    // ANSI reset code
    add_format_string(module, ctx, "\x1b[0m", "ansi_reset")?;
    // ANSI bold
    add_format_string(module, ctx, "\x1b[1m", "ansi_bold")?;
    // ANSI italic
    add_format_string(module, ctx, "\x1b[3m", "ansi_italic")?;
    // ANSI underline
    add_format_string(module, ctx, "\x1b[4m", "ansi_underline")?;
    // ANSI strikethrough
    add_format_string(module, ctx, "\x1b[9m", "ansi_strike")?;
    // "true" and "false" for boolean printing
    add_format_string(module, ctx, "true", "str_true")?;
    add_format_string(module, ctx, "false", "str_false")?;
    // Type names for typeof
    add_format_string(module, ctx, "number", "type_number")?;
    add_format_string(module, ctx, "bool", "type_bool")?;
    add_format_string(module, ctx, "string", "type_string")?;
    add_format_string(module, ctx, "object", "type_object")?;
    add_format_string(module, ctx, "pointer", "type_pointer")?;
    add_format_string(module, ctx, "unknown", "type_unknown")?;
    // Fixed-width type names
    add_format_string(module, ctx, "u8", "type_u8")?;
    add_format_string(module, ctx, "u16", "type_u16")?;
    add_format_string(module, ctx, "u32", "type_u32")?;
    add_format_string(module, ctx, "u64", "type_u64")?;
    add_format_string(module, ctx, "u128", "type_u128")?;
    add_format_string(module, ctx, "i8", "type_i8")?;
    add_format_string(module, ctx, "i16", "type_i16")?;
    add_format_string(module, ctx, "i32", "type_i32")?;
    add_format_string(module, ctx, "i64", "type_i64")?;
    add_format_string(module, ctx, "i128", "type_i128")?;
    add_format_string(module, ctx, "f32", "type_f32")?;
    add_format_string(module, ctx, "f64", "type_f64")?;
    add_format_string(module, ctx, "null", "str_null")?;
    add_format_string(module, ctx, "undefined", "str_undefined")?;
    // Pretty-print type hint suffixes
    add_format_string(module, ctx, " <bool>", "pp_bool")?;
    add_format_string(module, ctx, " <string>", "pp_string")?;
    add_format_string(module, ctx, " <number>", "pp_number")?;
    add_format_string(module, ctx, " <tuple>", "pp_tuple")?;
    add_format_string(module, ctx, " <any>", "pp_any")?;
    // ISO date string for Date.toISOString()
    add_format_string(module, ctx, "1970-01-01T00:00:00.000Z", "iso_date_string")?;
    // Array formatting strings
    add_format_string(module, ctx, "[", "arr_open")?;
    add_format_string(module, ctx, "]", "arr_close")?;
    add_format_string(module, ctx, ", ", "arr_sep")?;
    add_format_string(module, ctx, ", ...", "arr_ellipsis")?;
    add_format_string(module, ctx, "%d", "fmt_d")?;

    Ok(())
}
