//! Mechanic `recomp.pe32` — PE32 image parsing, exports by name and ordinal,
//! imports by name and ordinal, forwarders, and rejection of malformed images.
//!
//! The synthetic images are built here byte by byte so every offset in the
//! assertions is also a check on the parser's offset arithmetic. The final test
//! runs the same parser over the operator's extracted `War3.exe`, `Game.dll`,
//! `Storm.dll` and `ijl15.dll` and compares the census with `rabin2 6.2.6`
//! (independent reader, values recorded 2026-11-21); it skips when the
//! (untracked) analysis corpus is absent, as `real_corpus.rs` does.

use std::path::PathBuf;

use wc3_guest::pe::{
    PeError, PeImage, MAX_IMPORTS, MACHINE_I386, OPT_MAGIC_PE32, OPT_MAGIC_PE32_PLUS,
};

// Layout of `sample_pe` (two sections).
const PE_OFFSET: usize = 0x80;
const COFF: usize = PE_OFFSET + 4; // 0x84
const OPT: usize = COFF + 20; // 0x98
const SECTION_TABLE: usize = OPT + 0xE0; // 0x178

// Layout of `single_section_pe` (one section at RVA 0x1000, file 0x200).
const SS_PE_OFFSET: usize = 0x40;
const SS_COFF: usize = SS_PE_OFFSET + 4; // 0x44
const SS_OPT: usize = SS_COFF + 20; // 0x58
const SS_SECTION_TABLE: usize = SS_OPT + 0xE0; // 0x138
const SS_FILE_BASE: usize = 0x200;

fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_str(b: &mut [u8], off: usize, s: &str) {
    b[off..off + s.len()].copy_from_slice(s.as_bytes());
}

/// A minimal but complete PE32: two sections, one named + one ordinal-only
/// export, one import DLL with a named and an ordinal import.
fn sample_pe() -> Vec<u8> {
    let mut b = vec![0u8; 0x800];
    // DOS header.
    b[0] = b'M';
    b[1] = b'Z';
    put32(&mut b, 0x3C, PE_OFFSET as u32);
    // PE signature and COFF header.
    b[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    put16(&mut b, COFF, MACHINE_I386);
    put16(&mut b, COFF + 2, 2); // sections
    put16(&mut b, COFF + 16, 0x00E0); // optional header size
    put16(&mut b, COFF + 18, 0x0102); // executable | 32-bit
    // Optional header.
    put16(&mut b, OPT, OPT_MAGIC_PE32);
    put32(&mut b, OPT + 16, 0x1000); // AddressOfEntryPoint
    put32(&mut b, OPT + 28, 0x0040_0000); // ImageBase
    put32(&mut b, OPT + 32, 0x1000); // SectionAlignment
    put32(&mut b, OPT + 36, 0x200); // FileAlignment
    put32(&mut b, OPT + 56, 0x3000); // SizeOfImage
    put32(&mut b, OPT + 60, 0x200); // SizeOfHeaders
    put32(&mut b, OPT + 92, 16); // NumberOfRvaAndSizes
    put32(&mut b, OPT + 96, 0x2000); // export dir RVA
    put32(&mut b, OPT + 100, 0x100); // export dir size
    put32(&mut b, OPT + 104, 0x2100); // import dir RVA
    put32(&mut b, OPT + 108, 0x100); // import dir size
    // Section table.
    let text = SECTION_TABLE;
    put_str(&mut b, text, ".text");
    put32(&mut b, text + 8, 0x100); // virtual size
    put32(&mut b, text + 12, 0x1000); // virtual address
    put32(&mut b, text + 16, 0x200); // raw size
    put32(&mut b, text + 20, 0x200); // raw pointer
    let rdata = SECTION_TABLE + 40;
    put_str(&mut b, rdata, ".rdata");
    put32(&mut b, rdata + 8, 0x400);
    put32(&mut b, rdata + 12, 0x2000);
    put32(&mut b, rdata + 16, 0x400);
    put32(&mut b, rdata + 20, 0x400);
    // Export directory at RVA 0x2000 -> file 0x400.
    let exp = 0x400;
    put32(&mut b, exp + 16, 1); // Base
    put32(&mut b, exp + 20, 2); // NumberOfFunctions
    put32(&mut b, exp + 24, 1); // NumberOfNames
    put32(&mut b, exp + 28, 0x2040); // AddressOfFunctions
    put32(&mut b, exp + 32, 0x2050); // AddressOfNames
    put32(&mut b, exp + 36, 0x2060); // AddressOfNameOrdinals
    put32(&mut b, 0x440, 0x1000); // function[0]
    put32(&mut b, 0x444, 0x1010); // function[1] (ordinal-only export)
    put32(&mut b, 0x450, 0x2070); // name[0] -> "A1"
    put16(&mut b, 0x460, 0); // name ordinal index 0 -> function[0]
    put_str(&mut b, 0x470, "A1\0");
    // Import directory at RVA 0x2100 -> file 0x500.
    let imp = 0x500;
    put32(&mut b, imp, 0x2140); // OriginalFirstThunk
    put32(&mut b, imp + 12, 0x2180); // Name -> "KERNEL32.dll"
    put32(&mut b, imp + 16, 0x2140); // FirstThunk
    // Terminator is already zero.
    put32(&mut b, 0x540, 0x21A0); // import by name
    put32(&mut b, 0x544, 0x8000_0005); // import by ordinal 5
    // 0x548 stays zero: end of thunk list.
    put_str(&mut b, 0x580, "KERNEL32.dll\0");
    put16(&mut b, 0x5A0, 0); // hint
    put_str(&mut b, 0x5A2, "GetVersion\0");
    b
}

/// One-section PE32 (`.rdata` at RVA 0x1000, file 0x200) with room for
/// `raw_len` bytes of section data. Used by the import-table bounds tests.
fn single_section_pe(raw_len: usize) -> Vec<u8> {
    let mut b = vec![0u8; SS_FILE_BASE + raw_len];
    b[0..2].copy_from_slice(b"MZ");
    put32(&mut b, 0x3C, SS_PE_OFFSET as u32);
    b[SS_PE_OFFSET..SS_PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    put16(&mut b, SS_COFF, MACHINE_I386);
    put16(&mut b, SS_COFF + 2, 1);
    put16(&mut b, SS_COFF + 16, 0x00E0);
    put16(&mut b, SS_OPT, OPT_MAGIC_PE32);
    put32(&mut b, SS_OPT + 16, 0x1000);
    put32(&mut b, SS_OPT + 28, 0x0040_0000);
    put32(&mut b, SS_OPT + 56, 0x1000 + raw_len as u32);
    put32(&mut b, SS_OPT + 60, 0x200);
    put32(&mut b, SS_OPT + 92, 16);
    let section = SS_SECTION_TABLE;
    put_str(&mut b, section, ".rdata");
    put32(&mut b, section + 8, raw_len as u32);
    put32(&mut b, section + 12, 0x1000);
    put32(&mut b, section + 16, raw_len as u32);
    put32(&mut b, section + 20, SS_FILE_BASE as u32);
    b
}

/// File offset of an RVA in [`single_section_pe`].
fn section_off(rva: u32) -> usize {
    SS_FILE_BASE + (rva - 0x1000) as usize
}

/// `descriptors` import descriptors that all share one `thunks`-entry array.
/// Total flattened imports = `descriptors * thunks`; the parser must cap it.
fn shared_thunk_imports(descriptors: usize, thunks: usize) -> Vec<u8> {
    let mut b = single_section_pe(0x6000);
    put32(&mut b, SS_OPT + 104, 0x2000); // import dir RVA
    put32(&mut b, SS_OPT + 108, 0x100);
    put_str(&mut b, section_off(0x2200), "KERNEL32.dll\0");
    for i in 0..descriptors {
        let d = section_off(0x2000) + i * 20;
        put32(&mut b, d, 0x2F00); // OriginalFirstThunk
        put32(&mut b, d + 12, 0x2200); // Name
        put32(&mut b, d + 16, 0x2F00); // FirstThunk
    }
    // The descriptor terminator at 0x2000 + descriptors*20 stays zero.
    let start = section_off(0x2F00);
    for i in 0..thunks {
        put32(&mut b, start + i * 4, 0x8000_0001);
    }
    // The next word is zero: end of the shared thunk array.
    b
}

#[test]
fn recomp_pe32_parses_headers_and_sections() {
    let bytes = sample_pe();
    let image = PeImage::parse(&bytes).expect("sample parses");
    assert_eq!(image.machine, MACHINE_I386);
    assert_eq!(image.image_base, 0x0040_0000);
    assert_eq!(image.size_of_image, 0x3000);
    assert_eq!(image.entry_rva, 0x1000);
    assert_eq!(image.sections.len(), 2);
    assert_eq!(image.sections[0].name_str(), ".text");
    assert_eq!(image.sections[1].name_str(), ".rdata");
    assert_eq!(image.sections[1].virtual_address, 0x2000);
    // RVA -> file offset across headers and both sections.
    assert_eq!(image.rva_to_offset(0x40), Some(0x40), "header RVA maps 1:1");
    assert_eq!(image.rva_to_offset(0x1000), Some(0x200));
    assert_eq!(image.rva_to_offset(0x1100), Some(0x300));
    assert_eq!(image.rva_to_offset(0x2000), Some(0x400));
    assert_eq!(image.rva_to_offset(0x2100), Some(0x500));
    assert_eq!(image.rva_to_offset(0x2400), None, "past the section span");
    assert_eq!(image.rva_to_offset(0x9000), None, "unmapped RVA");
    assert_eq!(
        image.section_for_rva(0x2100).map(|s| s.name_str()),
        Some(".rdata".to_string())
    );
    assert!(image.section_for_rva(0x9000).is_none());
}

#[test]
fn recomp_pe32_exports_by_name_and_ordinal() {
    let bytes = sample_pe();
    let image = PeImage::parse(&bytes).expect("sample parses");
    assert_eq!(image.exports.len(), 2);
    let named = image.export_by_name("A1").expect("named export");
    assert_eq!(named.rva, 0x1000);
    assert_eq!(named.ordinal, 1);
    assert!(!named.is_forwarder());
    let ordinal_only = image.export_by_ordinal(2).expect("ordinal-only export");
    assert_eq!(ordinal_only.name, None);
    assert_eq!(ordinal_only.rva, 0x1010);
    assert_eq!(image.export_by_name("missing"), None);
    assert_eq!(image.export_by_ordinal(99), None);
}

#[test]
fn recomp_pe32_parses_forwarded_export() {
    // A third export whose RVA points inside the export directory is a
    // forwarder string, not code: a consumer must not map it as a function.
    let mut bytes = sample_pe();
    put32(&mut bytes, 0x400 + 20, 3); // NumberOfFunctions
    put32(&mut bytes, 0x448, 0x2080); // function[2] -> forwarder string
    put_str(&mut bytes, 0x480, "KERNEL32.Sleep\0");
    let image = PeImage::parse(&bytes).expect("forwarder image parses");
    assert_eq!(image.exports.len(), 3);
    let forwarded = image.export_by_ordinal(3).expect("third export");
    assert_eq!(forwarded.rva, 0x2080);
    assert_eq!(forwarded.forwarder.as_deref(), Some("KERNEL32.Sleep"));
    assert!(forwarded.is_forwarder());
    assert_eq!(forwarded.name, None);
    assert!(!image.export_by_ordinal(1).unwrap().is_forwarder());
}

#[test]
fn recomp_pe32_export_aliases_are_preserved() {
    // Two names may refer to the same function index; a census that keeps one
    // name per slot undercounts exports. Both aliases must appear.
    let mut bytes = sample_pe();
    put32(&mut bytes, 0x400 + 24, 2); // NumberOfNames
    put32(&mut bytes, 0x454, 0x2078); // second name -> "A1Alias"
    put16(&mut bytes, 0x462, 0); // same function index as the first name
    put_str(&mut bytes, 0x478, "A1Alias\0");
    let image = PeImage::parse(&bytes).expect("alias image parses");
    assert_eq!(image.exports.len(), 3, "two aliases + the ordinal-only slot");
    let names: Vec<&str> = image.exports.iter().filter_map(|e| e.name.as_deref()).collect();
    assert_eq!(names, vec!["A1", "A1Alias"]);
    for export in image.exports.iter().filter(|e| e.name.is_some()) {
        assert_eq!(export.ordinal, 1);
        assert_eq!(export.rva, 0x1000);
    }
    assert_eq!(image.export_by_name("A1Alias").map(|e| e.rva), Some(0x1000));
}

#[test]
fn recomp_pe32_imports_by_name_and_ordinal() {
    let bytes = sample_pe();
    let image = PeImage::parse(&bytes).expect("sample parses");
    assert_eq!(image.imports.len(), 2);
    assert_eq!(image.imports[0].module_str(), "KERNEL32.dll");
    assert_eq!(image.imports[0].name.as_deref(), Some("GetVersion"));
    assert_eq!(image.imports[0].ordinal, None);
    assert_eq!(image.imports[1].module_str(), "KERNEL32.dll");
    assert_eq!(image.imports[1].name, None);
    assert_eq!(image.imports[1].ordinal, Some(5));
    assert_eq!(image.import_modules(), vec!["KERNEL32.dll"]);
    // The module name is one allocation shared by every symbol of the DLL.
    assert!(std::sync::Arc::ptr_eq(&image.imports[0].module, &image.imports[1].module));
}

#[test]
fn recomp_pe32_import_cap_bounds_shared_thunk_arrays() {
    // Audit L547-01: several descriptors may share one thunk array, so the
    // per-descriptor caps are not a bound on the flattened result. 16 x 4095 is
    // the largest shared-array census under MAX_IMPORTS and must parse; a
    // 17th descriptor crosses the cap and must fail fast.
    let at_cap = shared_thunk_imports(16, 4095);
    assert!(16 * 4095 < MAX_IMPORTS && 17 * 4095 > MAX_IMPORTS);
    let image = PeImage::parse(&at_cap).expect("census under MAX_IMPORTS parses");
    assert_eq!(image.imports.len(), 16 * 4095);
    assert!(image.imports.iter().all(|i| i.ordinal == Some(1)));
    assert!(
        std::sync::Arc::ptr_eq(&image.imports[0].module, &image.imports[16 * 4095 - 1].module),
        "a shared module name is one allocation for the whole import table"
    );

    let over_cap = shared_thunk_imports(17, 4095);
    assert_eq!(
        PeImage::parse(&over_cap).unwrap_err(),
        PeError::BadImportDirectory,
        "the 65 537th import is a typed error, not an unbounded allocation"
    );
}

#[test]
fn recomp_pe32_import_name_budget_is_bounded() {
    // 1300 thunk entries point into one 4 KiB string at different offsets, so
    // every parse is a distinct suffix (~4.5 MiB of names from a ~20 KiB file).
    // Without a total budget the strings alone would multiply without bound;
    // with it the parse stops.
    let mut b = single_section_pe(0x5000);
    put32(&mut b, SS_OPT + 104, 0x2000);
    put32(&mut b, SS_OPT + 108, 0x100);
    let d = section_off(0x2000);
    put32(&mut b, d, 0x2200); // OriginalFirstThunk
    put32(&mut b, d + 12, 0x4100); // module name (past the 1301-entry thunk array)
    put32(&mut b, d + 16, 0x2200); // FirstThunk
    put_str(&mut b, section_off(0x4100), "K.dll\0");
    let thunks = section_off(0x2200);
    for i in 0..1300usize {
        put32(&mut b, thunks + i * 4, 0x3000 + i as u32);
    }
    // 0x3000..0x4000 is 'A', with the NUL at 0x4000.
    for i in 0..4096usize {
        b[section_off(0x3000) + i] = b'A';
    }
    assert_eq!(
        PeImage::parse(&b).unwrap_err(),
        PeError::BadImportDirectory,
        "the name budget stops the suffix amplification"
    );
}

#[test]
fn recomp_pe32_overlong_name_is_typed() {
    // A module name that does not terminate inside MAX_NAME_LEN is malformed.
    let mut b = single_section_pe(0x5000);
    put32(&mut b, SS_OPT + 104, 0x2000);
    put32(&mut b, SS_OPT + 108, 0x100);
    let d = section_off(0x2000);
    put32(&mut b, d, 0x2200);
    put32(&mut b, d + 12, 0x2800);
    put32(&mut b, d + 16, 0x2200);
    for i in 0..4096usize {
        b[section_off(0x2800) + i] = b'A';
    }
    assert_eq!(PeImage::parse(&b).unwrap_err(), PeError::BadImportDirectory);
}

#[test]
fn recomp_pe32_import_thunk_table_must_terminate() {
    // 4096 non-zero entries with no terminator: the loop window is exhausted
    // and the table is malformed — never silently truncated.
    let mut b = single_section_pe(0x4200);
    put32(&mut b, SS_OPT + 104, 0x2000);
    put32(&mut b, SS_OPT + 108, 0x100);
    let d = section_off(0x5000);
    put32(&mut b, d, 0x1000); // thunk array at the start of the section
    put32(&mut b, d + 12, 0x5100); // name
    put32(&mut b, d + 16, 0x1000);
    put_str(&mut b, section_off(0x5100), "K.dll\0");
    for i in 0..4096usize {
        put32(&mut b, section_off(0x1000) + i * 4, 0x8000_0005);
    }
    assert_eq!(PeImage::parse(&b).unwrap_err(), PeError::BadImportDirectory);
}

#[test]
fn recomp_pe32_rejects_malformed_images() {
    assert_eq!(PeImage::parse(&[]).unwrap_err(), PeError::TooShort);
    assert_eq!(PeImage::parse(&[0u8; 10]).unwrap_err(), PeError::TooShort);
    assert_eq!(PeImage::parse(&[0u8; 0x40]).unwrap_err(), PeError::BadDosMagic);

    let mut bad_signature = sample_pe();
    bad_signature[PE_OFFSET] = b'X';
    assert_eq!(PeImage::parse(&bad_signature).unwrap_err(), PeError::BadPeSignature);

    let mut pe32_plus = sample_pe();
    put16(&mut pe32_plus, OPT, OPT_MAGIC_PE32_PLUS);
    assert_eq!(PeImage::parse(&pe32_plus).unwrap_err(), PeError::NotPe32);

    let mut too_many_sections = sample_pe();
    put16(&mut too_many_sections, COFF + 2, 200);
    assert_eq!(PeImage::parse(&too_many_sections).unwrap_err(), PeError::BadSectionTable);

    let mut section_leaves_file = sample_pe();
    put32(&mut section_leaves_file, SECTION_TABLE + 16, 0x10_0000);
    assert_eq!(
        PeImage::parse(&section_leaves_file).unwrap_err(),
        PeError::SectionOutOfBounds
    );

    // Truncating the buffer must be an error, never a partial image.
    let full = sample_pe();
    for cut in [0x100usize, 0x200, 0x400, 0x600] {
        assert!(PeImage::parse(&full[..cut]).is_err(), "truncation at {cut:#x} must fail");
    }
}

/// The operator's extracted PE32 binaries (untracked analysis corpus).
fn shipped_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/game_files")
}

/// Parse a shipped image and run `check` against it. Returns `None` only when
/// the file is absent; a file that is present but unreadable or unparsable is a
/// test failure, not a skip.
fn census<T>(file: &str, check: impl FnOnce(&PeImage<'_>) -> T) -> Option<T> {
    let path = shipped_dir().join(file);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let image = PeImage::parse(&bytes)
        .unwrap_or_else(|error| panic!("parse {file}: {error:?}"));
    Some(check(&image))
}

fn imports_per_module(image: &PeImage<'_>) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for import in &image.imports {
        let module = import.module_str().to_ascii_lowercase();
        match counts.iter_mut().find(|(name, _)| *name == module) {
            Some((_, count)) => *count += 1,
            None => counts.push((module, 1)),
        }
    }
    counts.sort();
    counts
}

#[test]
fn recomp_pe32_shipped_host_ordinals_resolve() {
    // HLE ABI parity on the shipped pair: every ordinal `Game.dll` imports from
    // `War3.exe`/`Storm.dll` must be an export of that binary. This is the
    // ordinal-only host-service surface the loader has to resolve.
    if !shipped_dir().is_dir() {
        return;
    }
    let (Ok(war3_bytes), Ok(game_bytes), Ok(storm_bytes), Ok(ijl_bytes)) = (
        std::fs::read(shipped_dir().join("Files100_War3.exe")),
        std::fs::read(shipped_dir().join("Files100_Game.dll")),
        std::fs::read(shipped_dir().join("Files100_Storm.dll")),
        std::fs::read(shipped_dir().join("Files100_ijl15.dll")),
    ) else {
        return;
    };
    let war3 = PeImage::parse(&war3_bytes).expect("War3.exe parses");
    let game = PeImage::parse(&game_bytes).expect("Game.dll parses");
    let storm = PeImage::parse(&storm_bytes).expect("Storm.dll parses");
    let ijl = PeImage::parse(&ijl_bytes).expect("ijl15.dll parses");

    let mut host_ordinals = 0usize;
    let mut storm_ordinals = 0usize;
    let mut ijl_ordinals = 0usize;
    for import in &game.imports {
        let module = import.module_str().to_ascii_lowercase();
        match module.as_str() {
            "war3.exe" => {
                let ordinal = import.ordinal.expect("host import is by ordinal");
                let export = war3
                    .export_by_ordinal(ordinal)
                    .unwrap_or_else(|| panic!("War3.exe has no ordinal {ordinal}"));
                assert!(export.name.is_some(), "host export {ordinal} is named");
                assert!(!export.is_forwarder(), "host export {ordinal} is code");
                host_ordinals += 1;
            }
            "storm.dll" => {
                let ordinal = import.ordinal.expect("Storm import is by ordinal");
                assert!(
                    storm.export_by_ordinal(ordinal).is_some(),
                    "Storm.dll has no ordinal {ordinal}"
                );
                storm_ordinals += 1;
            }
            "ijl15.dll" => {
                let ordinal = import.ordinal.expect("ijl import is by ordinal");
                let export = ijl
                    .export_by_ordinal(ordinal)
                    .unwrap_or_else(|| panic!("ijl15.dll has no ordinal {ordinal}"));
                assert!(export.name.is_some(), "ijl15 export {ordinal} is named");
                ijl_ordinals += 1;
            }
            _ => assert!(import.ordinal.is_none(), "{module} imports by ordinal"),
        }
    }
    assert_eq!(host_ordinals, 644);
    assert_eq!(storm_ordinals, 92);
    assert_eq!(ijl_ordinals, 3);

    // War3.exe imports 68 services by ordinal: 43 Storm ordinals and 25
    // wsock32 ordinals (`rabin2` prints wsock32's names from its own database;
    // the file itself has ordinals, which is what the loader binds).
    let ordinal_imports: Vec<_> = war3.imports.iter().filter(|i| i.ordinal.is_some()).collect();
    assert_eq!(ordinal_imports.len(), 68);
    assert_eq!(
        ordinal_imports
            .iter()
            .filter(|i| i.module_str().eq_ignore_ascii_case("storm.dll"))
            .count(),
        43
    );
    assert_eq!(
        ordinal_imports
            .iter()
            .filter(|i| i.module_str().eq_ignore_ascii_case("wsock32.dll"))
            .count(),
        25
    );
}

#[test]
fn recomp_pe32_shipped_census_matches_rabin2() {
    // Oracle: `rabin2 6.2.6` (`-I`, `-S`, `-E`, `-i`) on the same files,
    // recorded 2026-11-21. The files are untracked copies of the operator's
    // licensed installation, so the test skips when they are absent.
    if !shipped_dir().is_dir() {
        return;
    }
    let Some(()) = census("Files100_War3.exe", |war3| {
        assert_eq!(war3.sections.len(), 7);
        assert_eq!(war3.image_base, 0x0040_0000);
        assert_eq!(war3.entry_rva, 0x67fdc);
        assert_eq!(war3.exports.len(), 695);
        assert_eq!(war3.imports.len(), 487);
        assert!(war3.exports.iter().all(|e| e.name.is_some() && !e.is_forwarder()));
        assert_eq!(war3.export_by_name("A1").map(|e| e.rva), Some(0x2dbe0));
        assert_eq!(war3.export_by_name("A1").map(|e| e.ordinal), Some(1));
        assert_eq!(war3.export_by_ordinal(695).and_then(|e| e.name.as_deref()), Some("LD"));
        assert_eq!(war3.import_modules().len(), 12);
    }) else {
        return;
    };

    let Some(()) = census("Files100_Game.dll", |game| {
        assert_eq!(game.sections.len(), 5);
        assert_eq!(game.image_base, 0x6f00_0000);
        assert_eq!(game.exports.len(), 1);
        assert_eq!(game.export_by_name("GameMain").map(|e| e.rva), Some(0x4180));
        assert_eq!(game.imports.len(), 931);
        assert_eq!(
            imports_per_module(game),
            vec![
                ("comctl32.dll".to_string(), 1),
                ("comdlg32.dll".to_string(), 1),
                ("gdi32.dll".to_string(), 14),
                ("ijl15.dll".to_string(), 3),
                ("imm32.dll".to_string(), 8),
                ("kernel32.dll".to_string(), 17),
                ("msvcrt.dll".to_string(), 60),
                ("ole32.dll".to_string(), 3),
                ("opengl32.dll".to_string(), 50),
                ("storm.dll".to_string(), 92),
                ("user32.dll".to_string(), 37),
                ("war3.exe".to_string(), 644),
                ("winmm.dll".to_string(), 1),
            ]
        );
    }) else {
        return;
    };

    let Some(()) = census("Files100_Storm.dll", |storm| {
        assert_eq!(storm.sections.len(), 5);
        assert_eq!(storm.exports.len(), 370);
        assert!(storm.exports.iter().all(|e| e.name.is_none()), "Storm exports are ordinal-only");
        assert_eq!(storm.exports.first().map(|e| e.ordinal), Some(101));
        assert_eq!(storm.exports.last().map(|e| e.ordinal), Some(647));
        assert_eq!(storm.imports.len(), 226);
    }) else {
        return;
    };

    let Some(()) = census("Files100_ijl15.dll", |ijl| {
        assert_eq!(ijl.sections.len(), 6);
        assert_eq!(ijl.exports.len(), 6);
        assert_eq!(ijl.export_by_name("ijlInit").map(|e| e.rva), Some(0x32300));
        assert_eq!(ijl.imports.len(), 59);
    }) else {
        return;
    };
}
