use crate::execution::runtime::pretty_print::{PrettyPrintOptions, pretty_print};
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::io::Write as IoWrite;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "print",
        "core",
        "Print values to stdout with optional formatting",
        builtin_print,
    );
    registry.register(
        "println",
        "core",
        "Print values to stdout with newline",
        builtin_println,
    );
    registry.register("eprint", "core", "Print values to stderr", builtin_eprint);
}

fn builtin_print(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    use std::io::BufWriter;

    let (values, options) = parse_print_options(&args);

    let sep = options
        .get("sep")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or(" ");
    let end = options
        .get("end")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("\n");
    let color = options.get("color").and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });
    let background = options.get("background").and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });
    let underline = options
        .get("underline")
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(false);
    let bold = options
        .get("bold")
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(false);
    let italic = options
        .get("italic")
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(false);
    let strikethrough = options
        .get("strikethrough")
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(false);

    // Check for pretty printing mode
    let pretty = options.get("pretty").and_then(|v| match v {
        Value::Bool(b) => Some(if *b { "full" } else { "none" }),
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });

    // First, build the output string
    let mut output_str = String::with_capacity(256);

    // Check file output first to avoid formatting if not needed
    if let Some(Value::Str(file)) = options.get("file") {
        for (i, val) in values.iter().enumerate() {
            if i > 0 {
                output_str.push_str(sep);
            }
            output_str.push_str(&crate::execution::runtime::format::fmt(val));
        }
        output_str.push_str(end);
        write_to_file(file, &output_str)?;
        return Ok(Value::Null);
    }

    // Build output for both stdout and test capture
    if let Some(pretty_mode) = pretty {
        if pretty_mode != "none" {
            let pretty_opts = match pretty_mode {
                "compact" => PrettyPrintOptions::compact(),
                "simple" => PrettyPrintOptions::simple_color(),
                "full" | _ => PrettyPrintOptions::default(),
            };

            for (i, val) in values.iter().enumerate() {
                if i > 0 {
                    output_str.push_str(sep);
                }
                let pretty_output = pretty_print(val, &pretty_opts);
                output_str.push_str(&pretty_output);
            }
            output_str.push_str(end);
        }
    } else {
        // Build output normally
        for (i, val) in values.iter().enumerate() {
            if i > 0 {
                output_str.push_str(sep);
            }
            output_str.push_str(&crate::execution::runtime::format::fmt(val));
        }
        output_str.push_str(end);
    }

    // Write to stdout
    let stdout = std::io::stdout();
    let mut writer = BufWriter::with_capacity(8192, stdout.lock());

    // Check if styling is needed
    let needs_styling =
        color.is_some() || background.is_some() || underline || bold || italic || strikethrough;

    if needs_styling {
        // Apply styles (minimal allocation)
        write_styled(
            &mut writer,
            values,
            sep,
            end,
            color,
            background,
            underline,
            bold,
            italic,
            strikethrough,
        )?;
    } else {
        writer
            .write_all(output_str.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    writer.flush().map_err(|e| e.to_string())?;
    let _ = std::io::stdout().flush();
    Ok(Value::Null)
}

fn builtin_println(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    // call print with end="\n"
    let mut xs = args.clone();
    let mut opts: HashMap<String, Value> = HashMap::default();
    opts.insert("end".to_string(), Value::Str("\n".to_string()));
    xs.push(Value::Object(std::sync::Arc::new(opts)));
    builtin_print(env, xs)
}

fn builtin_eprint(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let (values, _options) = parse_print_options(&args);
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            eprint!(" ");
        }
        eprint!("{}", crate::execution::runtime::format::fmt(val));
    }
    eprintln!();
    Ok(Value::Null)
}

fn parse_print_options(args: &[Value]) -> (&[Value], HashMap<String, Value>) {
    if let Some(Value::Object(opts)) = args.last() {
        let is_options = opts.contains_key("sep")
            || opts.contains_key("end")
            || opts.contains_key("file")
            || opts.contains_key("color")
            || opts.contains_key("background")
            || opts.contains_key("underline")
            || opts.contains_key("bold")
            || opts.contains_key("italic")
            || opts.contains_key("strikethrough")
            || opts.contains_key("pretty");
        if is_options {
            let values = &args[..args.len() - 1];
            let options = (**opts).clone();
            return (values, options);
        }
    }
    (args, HashMap::default())
}

/// Write values with ANSI styling directly to writer (zero-copy when possible)
fn write_styled<W: std::io::Write>(
    writer: &mut W,
    values: &[Value],
    sep: &str,
    end: &str,
    color: Option<&str>,
    background: Option<&str>,
    underline: bool,
    bold: bool,
    italic: bool,
    strikethrough: bool,
) -> Result<(), String> {
    // Build ANSI codes once
    let mut codes = Vec::with_capacity(6);
    if bold {
        codes.push("1");
    }
    if italic {
        codes.push("3");
    }
    if underline {
        codes.push("4");
    }
    if strikethrough {
        codes.push("9");
    }

    let color_code = if let Some(hex) = color {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            format!("38;2;{};{};{}", r, g, b)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    if !color_code.is_empty() {
        codes.push(&color_code);
    }

    let bg_code = if let Some(hex) = background {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            format!("48;2;{};{};{}", r, g, b)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    if !bg_code.is_empty() {
        codes.push(&bg_code);
    }

    // Write ANSI prefix once
    if !codes.is_empty() {
        write!(writer, "\x1b[{}m", codes.join(";")).map_err(|e| e.to_string())?;
    }

    // Write values
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            writer
                .write_all(sep.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        writer
            .write_all(crate::execution::runtime::format::fmt(val).as_bytes())
            .map_err(|e| e.to_string())?;
    }
    writer
        .write_all(end.as_bytes())
        .map_err(|e| e.to_string())?;

    // Write ANSI reset
    if !codes.is_empty() {
        writer.write_all(b"\x1b[0m").map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Apply ANSI styling to text
#[allow(dead_code)]
fn apply_styles(
    text: &str,
    color: &Option<String>,
    background: &Option<String>,
    underline: bool,
    bold: bool,
    italic: bool,
    strikethrough: bool,
) -> String {
    let mut codes: Vec<String> = Vec::new();

    // Text formatting codes
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

    // Foreground color (text color)
    if let Some(hex) = color {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            codes.push(format!("38;2;{};{};{}", r, g, b));
        }
    }

    // Background color
    if let Some(hex) = background {
        if let Some((r, g, b)) = parse_hex_color(hex) {
            codes.push(format!("48;2;{};{};{}", r, g, b));
        }
    }

    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// Parse hex color string to RGB values
/// Supports both 6-character (#RRGGBB) and 3-character (#RGB) formats
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
        // Handle shorthand #RGB format - expand to #RRGGBB
        let chars: Vec<char> = hex.chars().collect();
        let r_str: String = [chars[0], chars[0]].iter().collect();
        let g_str: String = [chars[1], chars[1]].iter().collect();
        let b_str: String = [chars[2], chars[2]].iter().collect();
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

fn write_to_file(path: &str, content: &str) -> Result<(), String> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write to file: {}", e))?;
    Ok(())
}
