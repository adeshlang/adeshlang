//! Diagnostic probe: does the runtime archive's symbol index expose the
//! LTO-promoted `RuntimeValue::clone` symbol, and does ingesting its defining
//! member resolve a weak reference to it?

use adesh_linker::archive::Archive;
use adesh_linker::resolver::SymbolResolver;
use adesh_linker::target::Target;
use std::path::PathBuf;

const CLONE_SUFFIX: &str =
    "5clone17h65240ebec6874242E.llvm.9256855803343597804";

#[test]
fn probe_aot_make_string_indexed() {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib");
    let bytes = std::fs::read(&lib).expect("read runtime archive");
    let target = Target::host();
    let archive =
        Archive::parse_with_target(&bytes, &lib, &target).expect("parse archive");
    let hits = archive
        .symbol_index
        .keys()
        .filter(|k| {
            k.contains("aot_make_string")
                || k.contains("aot_slice_array")
                || k.contains("aot_object_spread")
        })
        .cloned()
        .collect::<Vec<String>>();
    println!("aot export index keys: {hits:?}");
    assert!(
        hits.iter().any(|k| k == "aot_make_string"),
        "plain export `aot_make_string` missing from archive symbol index"
    );
}

#[test]
fn probe_promoted_clone_resolution() {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib");
    let bytes = std::fs::read(&lib).expect("read runtime archive");
    let target = Target::host();
    let archive =
        Archive::parse_with_target(&bytes, &lib, &target).expect("parse archive");

    let exact: Vec<String> = archive
        .symbol_index
        .keys()
        .filter(|k| k.ends_with(CLONE_SUFFIX))
        .cloned()
        .collect();
    println!("exact-indexed promoted names: {exact:?}");

    let base = CLONE_SUFFIX.split(".llvm.").next().unwrap();
    let fuzzy: Vec<String> = archive
        .symbol_index
        .keys()
        .filter(|k| k.contains(base))
        .cloned()
        .collect();
    println!("fuzzy-indexed clone names: {fuzzy:?}");
    assert!(
        !fuzzy.is_empty(),
        "archive symbol index has no entry for the promoted clone"
    );

    // Ingest every defining member of every clone variant and see what the
    // resolver ends up with for the promoted name.
    for key in &fuzzy {
        let members = archive.symbol_index.get(key).unwrap().clone();
        for m_idx in members {
            let member = &archive.members[m_idx];
            let mut obj = member.obj.clone().unwrap_or_else(|| {
                adesh_linker::object::reader::ObjectReader::read_from_memory(
                    &member.data,
                    std::path::Path::new(&member.name),
                    &target,
                    0,
                )
                .expect("read member")
            });
            obj.reassign_file_index(0);
            let symbols: Vec<String> = obj
                .symbols
                .iter()
                .filter(|s| s.name.contains("clone17h65240"))
                .map(|s| {
                    format!(
                        "name={} defined={} local={} binding={:?} sec={:?}",
                        s.name,
                        s.is_defined,
                        s.is_local(),
                        s.binding,
                        s.section_index
                    )
                })
                .collect();
            println!("member {} ({}): {symbols:#?}", member.name, m_idx);
        }
    }

    // End-to-end: resolve an undefined reference to the promoted name against
    // the archive and confirm it lands on a definition.
    let mut resolver = SymbolResolver::new();
    let mut objects: Vec<adesh_linker::object::ObjectFile> = Vec::new();
    // Simulate a referencing object by registering the undefined symbol the
    // way a weak external would be classified after binding.
    resolver.weak_undefined_symbols.insert(format!(
        "_ZN66_$LT$adesh_runtime..RuntimeValue$u20$as$u20$core..clone..Clone$GT${CLONE_SUFFIX}"
    ));
    let mut archives = vec![archive];
    let _ = resolver.resolve_with_target(&mut objects, &mut archives, &target);
    for name in resolver.weak_undefined_symbols.iter() {
        if name.ends_with(CLONE_SUFFIX) {
            let resolved = resolver.table.get(name);
            println!("post-resolve {name}: {resolved:?}");
            assert!(
                resolved.map(|r| r.symbol.is_defined).unwrap_or(false)
                    && resolved.unwrap().symbol.section_index.is_some(),
                "promoted clone still not bound to a definition"
            );
        }
    }
}
