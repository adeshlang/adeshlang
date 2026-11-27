//! `adesh target [list|info|features|capabilities]` command rendering.
//!
//! Queries the unified `adesh_object::TargetDescriptor` / `ArchitectureDescriptor`
//! and `adesh_linker::target::Target` tier table so every target subcommand reports
//! honest, non-conflated ISA, width, memory-model, container, runtime-profile,
//! capability, and verification-tier metadata.

use adesh_linker::target::{Target as LinkerTarget, advertised_by_tier};
use adesh_object::{TargetCapabilities, TargetDescriptor};

pub fn run_target_subcommand(args: &[String]) -> Result<(), String> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");
    match subcommand {
        "list" => {
            println!("{}", render_target_list());
            Ok(())
        }
        "info" => {
            let triple = args
                .get(3)
                .ok_or_else(|| "Usage: adesh target info <TRIPLE>".to_string())?;
            let out = render_target_info(triple)?;
            println!("{}", out);
            Ok(())
        }
        "features" => {
            let triple = args
                .get(3)
                .ok_or_else(|| "Usage: adesh target features <TRIPLE>".to_string())?;
            let out = render_target_features(triple)?;
            println!("{}", out);
            Ok(())
        }
        "capabilities" => {
            let triple = args
                .get(3)
                .ok_or_else(|| "Usage: adesh target capabilities <TRIPLE>".to_string())?;
            let out = render_target_capabilities(triple)?;
            println!("{}", out);
            Ok(())
        }
        other => Err(format!(
            "Unknown target subcommand: `{}`\nUsage: adesh target [list|info <TRIPLE>|features <TRIPLE>|capabilities <TRIPLE>]",
            other
        )),
    }
}

pub fn render_target_list() -> String {
    let mut out = String::new();
    for (tier, targets) in advertised_by_tier() {
        out.push_str(tier.advertised_heading());
        out.push_str(":\n");
        for entry in targets {
            out.push_str(&format!("    {:<30} {}\n", entry.triple, entry.summary));
        }
        out.push('\n');
    }
    out.push_str(
        "Use: adesh build --target=<TRIPLE> <file.adesh>\n\
         Inspect target details: adesh target info <TRIPLE> | features <TRIPLE> | capabilities <TRIPLE>\n\
         Per-target gaps and capability limits: TARGET_MATRIX.md (canonical status: CURRENT_STATE.md).",
    );
    out
}

pub fn render_target_info(triple: &str) -> Result<String, String> {
    let obj_desc = TargetDescriptor::from_triple(triple)
        .map_err(|e| format!("Invalid target triple `{}`: {}", triple, e))?;
    let arch_desc = obj_desc.arch_descriptor();
    let linker_tier = LinkerTarget::from_triple(triple)
        .map(|t| t.tier().as_str().to_string())
        .unwrap_or_else(|_| "Tier 3 (DECLARED / linker unsupported)".to_string());

    let obj_formats: Vec<&str> = arch_desc
        .object_formats
        .iter()
        .map(|f| match f {
            adesh_object::ObjectFormat::Elf => "ELF",
            adesh_object::ObjectFormat::PeCoff => "PE/COFF",
            adesh_object::ObjectFormat::MachO => "Mach-O",
            adesh_object::ObjectFormat::Wasm => "WASM",
            adesh_object::ObjectFormat::Adob => "ADOB",
            adesh_object::ObjectFormat::RawBinary => "Raw/Flat",
        })
        .collect();
    let exe_formats: Vec<&str> = arch_desc
        .executable_formats
        .iter()
        .map(|f| f.as_str())
        .collect();

    Ok(format!(
        "Target: {}\n\
         Canonical Triple:   {}\n\
         CPU Architecture:   {} ({})\n\
         Instruction Set:    {}\n\
         Register Width:     {}-bit (native word: {}-bit, GPRs: {})\n\
         Pointer Width:      {}-bit\n\
         Address Width:      {}-bit (max directly addressable: {} bytes)\n\
         Data Bus Width:     {}-bit (min addressable unit: {}-bit)\n\
         Instruction Width:  {}-{} bits\n\
         Endianness:         {:?}\n\
         Memory Model:       {}\n\
         Object Format(s):   {}\n\
         Executable Format:  {}\n\
         Calling Convention: {}\n\
         Runtime Profile:    {}\n\
         Verification Tier:  {}",
        triple,
        obj_desc.triple_string(),
        arch_desc.architecture_name,
        arch_desc.architecture_id,
        arch_desc.isa.as_str(),
        arch_desc.register_width_bits,
        arch_desc.native_word_bits,
        arch_desc.gpr_count,
        arch_desc.pointer_width_bits,
        arch_desc.address_width_bits,
        arch_desc.max_addressable_bytes,
        arch_desc.data_width_bits,
        arch_desc.data_layout.min_addressable_unit_bits,
        arch_desc.instruction_width_bits.0,
        arch_desc.instruction_width_bits.1,
        arch_desc.endianness,
        arch_desc.memory_model.as_str(),
        obj_formats.join(", "),
        exe_formats.join(", "),
        arch_desc.calling_conventions.join(", "),
        arch_desc.runtime_profile.as_str(),
        linker_tier
    ))
}

pub fn render_target_features(triple: &str) -> Result<String, String> {
    let obj_desc = TargetDescriptor::from_triple(triple)
        .map_err(|e| format!("Invalid target triple `{}`: {}", triple, e))?;
    let arch_desc = obj_desc.arch_descriptor();
    let caps = TargetCapabilities::for_descriptor(arch_desc);

    let mut enabled = Vec::new();
    let mut disabled = Vec::new();
    for feat in &caps.features.features {
        if feat.enabled {
            enabled.push(feat.name.as_str());
        } else {
            disabled.push(feat.name.as_str());
        }
    }

    Ok(format!(
        "Target Features: {} ({})\n\
         Supported Data Types:  {}\n\
         Hardware Multiply:     {}\n\
         Hardware Divide:       {}\n\
         Hardware Float (FPU):  {}\n\
         Enabled ISA Features:  {}\n\
         Disabled ISA Features: {}",
        triple,
        arch_desc.isa.as_str(),
        arch_desc.supported_data_types.join(", "),
        yes_no(arch_desc.hardware_multiply),
        yes_no(arch_desc.hardware_divide),
        yes_no(arch_desc.hardware_float),
        if enabled.is_empty() {
            "none".to_string()
        } else {
            enabled.join(", ")
        },
        if disabled.is_empty() {
            "none".to_string()
        } else {
            disabled.join(", ")
        },
    ))
}

pub fn render_target_capabilities(triple: &str) -> Result<String, String> {
    let obj_desc = TargetDescriptor::from_triple(triple)
        .map_err(|e| format!("Invalid target triple `{}`: {}", triple, e))?;
    let arch_desc = obj_desc.arch_descriptor();
    let rt = &arch_desc.runtime_capabilities;
    let layout = &arch_desc.data_layout;
    let linker_tier = LinkerTarget::from_triple(triple)
        .map(|t| t.tier().as_str().to_string())
        .unwrap_or_else(|_| "Tier 3 (DECLARED)".to_string());

    let mut supported = Vec::new();
    let mut unsupported = Vec::new();
    for (name, flag) in [
        ("allocation", rt.allocation),
        ("deallocation", rt.deallocation),
        ("printing", rt.printing),
        ("file_io", rt.file_io),
        ("networking", rt.networking),
        ("threads", rt.threads),
        ("atomics", rt.atomics),
        ("tls", rt.tls),
        ("exceptions", rt.exceptions),
        ("async", rt.async_runtime),
        ("time", rt.time),
        ("randomness", rt.randomness),
    ] {
        if flag {
            supported.push(name);
        } else {
            unsupported.push(name);
        }
    }

    Ok(format!(
        "Target Capabilities: {} ({})\n\
         Verification Tier:     {}\n\
         Runtime Profile:       {}\n\
         Supported Features:    {}\n\
         Unsupported Features:  {}\n\
         Data Layout:           ptr={}b (align {}), usize={}b, unaligned_ok={}, min_unit={}b",
        triple,
        arch_desc.architecture_name,
        linker_tier,
        arch_desc.runtime_profile.as_str(),
        if supported.is_empty() {
            "none".to_string()
        } else {
            supported.join(", ")
        },
        if unsupported.is_empty() {
            "none".to_string()
        } else {
            unsupported.join(", ")
        },
        layout.pointer_width_bits,
        layout.pointer_align,
        layout.usize_bits,
        yes_no(layout.unaligned_access_allowed),
        layout.min_addressable_unit_bits,
    ))
}

fn yes_no(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_info_distinguishes_8_16_24_32_64_bit_architectures() {
        let avr = render_target_info("avr-unknown-unknown").expect("avr info");
        assert!(avr.contains("Register Width:     8-bit"));
        assert!(avr.contains("Pointer Width:      16-bit"));
        assert!(avr.contains("Object Format(s):   ADOB, ELF"));
        assert!(avr.contains("harvard-flash-sram"));

        let dos = render_target_info("i8086-unknown-dos").expect("i8086 info");
        assert!(dos.contains("Register Width:     16-bit"));
        assert!(dos.contains("Address Width:      20-bit"));
        assert!(dos.contains("segmented-real-mode"));

        let dsp = render_target_info("dsp56000-unknown-unknown").expect("dsp56000 info");
        assert!(dsp.contains("Register Width:     24-bit"));
        assert!(dsp.contains("min addressable unit: 24-bit"));

        let i686 = render_target_info("i686-unknown-linux-gnu").expect("i686 info");
        assert!(i686.contains("Register Width:     32-bit"));

        let x64 = render_target_info("x86_64-unknown-linux-gnu").expect("x86_64 info");
        assert!(x64.contains("Register Width:     64-bit"));
    }

    #[test]
    fn target_capabilities_report_supported_and_unsupported_explicitly() {
        let avr_caps = render_target_capabilities("avr-unknown-unknown").expect("avr caps");
        assert!(avr_caps.contains("Bare-Metal Embedded"));
        assert!(avr_caps.contains("Unsupported Features:"));
        assert!(avr_caps.contains("threads"));
        assert!(avr_caps.contains("networking"));

        let avr_feats = render_target_features("avr-unknown-unknown").expect("avr feats");
        assert!(avr_feats.contains("Hardware Float (FPU):  no"));
    }
}
