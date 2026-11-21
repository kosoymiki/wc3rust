//! Census a PE32 image: section table, exports by name/ordinal, imports.
//!
//! The parser reads the operator's own licensed files and prints what it found;
//! nothing is written and nothing is bundled.
//!
//! ```sh
//! cargo run -p wc3-guest --example pe_inspect -- "/path/to/war3.exe"
//! ```

use std::process::ExitCode;

use wc3_guest::pe::PeImage;

fn main() -> ExitCode {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: pe_inspect <file.exe|file.dll>");
            return ExitCode::from(2);
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {path}: {err}");
            return ExitCode::from(2);
        }
    };
    let image = match PeImage::parse(&bytes) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("parse {path}: {err:?}");
            return ExitCode::FAILURE;
        }
    };

    println!("file          {path}");
    println!("bytes         {}", bytes.len());
    println!("machine       {:#06x}", image.machine);
    println!("image_base    {:#010x}", image.image_base);
    println!("size_of_image {:#x}", image.size_of_image);
    println!("entry_rva     {:#x}", image.entry_rva);
    println!("sections      {}", image.sections.len());
    for section in &image.sections {
        println!(
            "  {:<8} va={:#010x} vsize={:#08x} raw={:#08x}@{:#08x}",
            section.name_str(),
            section.virtual_address,
            section.virtual_size,
            section.raw_size,
            section.raw_pointer
        );
    }

    println!("exports       {}", image.exports.len());
    let mut named = 0usize;
    let mut ordinal_only = 0usize;
    let mut forwarders = 0usize;
    for export in &image.exports {
        if export.name.is_some() {
            named += 1;
        } else {
            ordinal_only += 1;
        }
        if export.is_forwarder() {
            forwarders += 1;
        }
    }
    println!("  named={named} ordinal_only={ordinal_only} forwarders={forwarders}");
    for export in image.exports.iter().take(12) {
        let name = match &export.name {
            Some(name) => name.clone(),
            None => "<ordinal>".to_string(),
        };
        match &export.forwarder {
            Some(target) => println!("  #{} {name} -> {target}", export.ordinal),
            None => println!("  #{} {name} @ {:#x}", export.ordinal, export.rva),
        }
    }
    if image.exports.len() > 12 {
        println!("  … {} more", image.exports.len() - 12);
    }

    println!("imports       {}", image.imports.len());
    let modules = image.import_modules();
    println!("  modules={} {:?}", modules.len(), modules);
    for import in image.imports.iter().take(12) {
        match (&import.name, import.ordinal) {
            (Some(name), _) => println!("  {}!{name}", import.module_str()),
            (None, Some(ordinal)) => println!("  {}!#{ordinal}", import.module_str()),
            (None, None) => println!("  {}!<nameless>", import.module_str()),
        }
    }
    if image.imports.len() > 12 {
        println!("  … {} more", image.imports.len() - 12);
    }
    ExitCode::SUCCESS
}
