use std::fs;
use std::path::Path;

#[test]
fn test_memory_examples_diagnostics() {
    let dir = Path::new("examples/memory");
    let mut total = 0;
    let mut parse_errors = 0;
    let mut type_errors = 0;

    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("adesh") {
            total += 1;
            let content = fs::read_to_string(&path).unwrap();
            let path_str = path.to_string_lossy().to_string();

            // 1. Check semantics index
            let index = adeshlang::semantics::index_source_in(&content, Some(&path_str));
            if !index.errors.is_empty() {
                parse_errors += 1;
                eprintln!("[PARSE/SEMANTIC ERROR] {}: {:?}", path.file_name().unwrap().to_string_lossy(), index.errors);
            }

            // 2. Check typesystem
            if let Err(e) = adeshlang::typesystem::type_system::check_module_in(&content, Some(&path_str)) {
                type_errors += 1;
                eprintln!("[TYPE ERROR] {}: {}", path.file_name().unwrap().to_string_lossy(), e);
            }
        }
    }

    eprintln!("\nSUMMARY: Total: {}, Parse Errors: {}, Type Errors: {}", total, parse_errors, type_errors);
}
