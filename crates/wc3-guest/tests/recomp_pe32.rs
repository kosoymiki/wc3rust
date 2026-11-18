//! Mechanic `recomp.pe32` — PE32 image parsing, exports by name and ordinal,
//! imports by name and ordinal, and rejection of malformed images.
//!
//! The image is built here byte by byte so every offset in the assertions is
//! also a check on the parser's offset arithmetic. (No PE is bundled: real-file
//! parsing is exercised by `examples/pe_inspect.rs` against the operator's own
//! licensed copy.)

use wc3_guest::pe::{PeError, PeImage, MACHINE_I386, OPT_MAGIC_PE32, OPT_MAGIC_PE32_PLUS};

const FILE_SIZE: usize = 0x800;
const PE_OFFSET: usize = 0x80;
const OPT: usize = PE_OFFSET + 4 + 20; // 0x98
const SECTION_TABLE: usize = OPT + 0xE0; // 0x178

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
    let mut b = vec![0u8; FILE_SIZE];
    // DOS header.
    b[0] = b'M';
    b[1] = b'Z';
    put32(&mut b, 0x3C, PE_OFFSET as u32);
    // PE signature and COFF header.
    b[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    put16(&mut b, PE_OFFSET + 4, MACHINE_I386);
    put16(&mut b, PE_OFFSET + 6, 2); // sections
    put16(&mut b, PE_OFFSET + 20, 0x00E0); // optional header size
    put16(&mut b, PE_OFFSET + 22, 0x0102); // executable | 32-bit
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
}

#[test]
fn recomp_pe32_exports_by_name_and_ordinal() {
    let bytes = sample_pe();
    let image = PeImage::parse(&bytes).expect("sample parses");
    assert_eq!(image.exports.len(), 2);
    let named = image.export_by_name("A1").expect("named export");
    assert_eq!(named.rva, 0x1000);
    assert_eq!(named.ordinal, 1);
    let ordinal_only = image.export_by_ordinal(2).expect("ordinal-only export");
    assert_eq!(ordinal_only.name, None);
    assert_eq!(ordinal_only.rva, 0x1010);
    assert_eq!(image.export_by_name("missing"), None);
    assert_eq!(image.export_by_ordinal(99), None);
}

#[test]
fn recomp_pe32_imports_by_name_and_ordinal() {
    let bytes = sample_pe();
    let image = PeImage::parse(&bytes).expect("sample parses");
    assert_eq!(image.imports.len(), 2);
    assert_eq!(image.imports[0].module, "KERNEL32.dll");
    assert_eq!(image.imports[0].name.as_deref(), Some("GetVersion"));
    assert_eq!(image.imports[0].ordinal, None);
    assert_eq!(image.imports[1].module, "KERNEL32.dll");
    assert_eq!(image.imports[1].name, None);
    assert_eq!(image.imports[1].ordinal, Some(5));
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
    put16(&mut too_many_sections, PE_OFFSET + 6, 200);
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
