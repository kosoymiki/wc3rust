//! Bounds-checked PE32 image parser.
//!
//! Ported from `runtime/guest/pe.c` of NIK2703/WC3_AndroidRecomp (MIT, commit
//! `c753ab89…`). The donor maps the licensed image at its preferred base and
//! resolves exports by name **and ordinal** because `War3.exe` exports
//! ordinal-only `A<n>` host services. This module keeps the parsing half: it
//! never maps memory and never writes.
//!
//! Every read goes through a checked reader, so a truncated or hostile file
//! returns [`PeError`] instead of panicking. The parser is deliberately PE32
//! (magic `0x10B`) only — PE32+ is rejected rather than half-supported.

/// `IMAGE_FILE_MACHINE_I386`.
pub const MACHINE_I386: u16 = 0x014C;
/// `IMAGE_NT_OPTIONAL_HDR32_MAGIC`.
pub const OPT_MAGIC_PE32: u16 = 0x010B;
/// `IMAGE_NT_OPTIONAL_HDR64_MAGIC` (rejected).
pub const OPT_MAGIC_PE32_PLUS: u16 = 0x020B;

/// Why an image could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeError {
    /// The buffer ends before a required field.
    TooShort,
    /// Missing `MZ`.
    BadDosMagic,
    /// Missing `PE\0\0` at `e_lfanew`.
    BadPeSignature,
    /// Optional header magic is not [`OPT_MAGIC_PE32`].
    NotPe32,
    /// Section table does not fit the buffer.
    BadSectionTable,
    /// A section's raw data leaves the buffer.
    SectionOutOfBounds,
    /// Export directory is inconsistent.
    BadExportDirectory,
    /// Import directory is inconsistent.
    BadImportDirectory,
}

/// One section header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Raw 8-byte name, NUL-padded.
    pub name: [u8; 8],
    /// RVA of the section.
    pub virtual_address: u32,
    /// Virtual size.
    pub virtual_size: u32,
    /// Size of the raw data on disk.
    pub raw_size: u32,
    /// File offset of the raw data.
    pub raw_pointer: u32,
    /// `IMAGE_SCN_*` characteristics.
    pub characteristics: u32,
}

impl Section {
    /// The name with trailing NULs/padding removed, lossily decoded.
    pub fn name_str(&self) -> String {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(self.name.len());
        String::from_utf8_lossy(&self.name[..end]).into_owned()
    }

    /// Span used for RVA mapping: `max(virtual_size, raw_size)`.
    pub fn span(&self) -> u32 {
        self.virtual_size.max(self.raw_size)
    }

    /// True when `rva` falls inside this section.
    pub fn contains_rva(&self, rva: u32) -> bool {
        rva >= self.virtual_address && rva < self.virtual_address.saturating_add(self.span())
    }
}

/// One exported symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// Name when the export is named, `None` for ordinal-only exports.
    pub name: Option<String>,
    /// Ordinal (export directory base + index).
    pub ordinal: u16,
    /// RVA of the export.
    pub rva: u32,
}

/// One imported symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// Guest DLL name as written in the import descriptor.
    pub module: String,
    /// Import name, `None` when imported by ordinal.
    pub name: Option<String>,
    /// Import ordinal when imported by ordinal.
    pub ordinal: Option<u16>,
}

/// A parsed PE32 image.
#[derive(Clone, Debug)]
pub struct PeImage<'a> {
    /// `IMAGE_FILE_HEADER.Machine`.
    pub machine: u16,
    /// Preferred load address.
    pub image_base: u32,
    /// `SizeOfImage`.
    pub size_of_image: u32,
    /// `AddressOfEntryPoint` (RVA).
    pub entry_rva: u32,
    /// Sections in table order.
    pub sections: Vec<Section>,
    /// Export directory entries, ordered by export index.
    pub exports: Vec<Export>,
    /// Import descriptors, flattened to one entry per symbol.
    pub imports: Vec<Import>,
    size_of_headers: u32,
    bytes: &'a [u8],
}

impl<'a> PeImage<'a> {
    /// Parse a PE32 image from a byte slice.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PeError> {
        if bytes.len() < 0x40 {
            return Err(PeError::TooShort);
        }
        if &bytes[0..2] != b"MZ" {
            return Err(PeError::BadDosMagic);
        }
        let r = Reader { bytes };
        let pe_offset = r.u32(0x3C)? as usize;
        match r.bytes(pe_offset, 4) {
            None => return Err(PeError::TooShort),
            Some(signature) if signature != b"PE\0\0" => {
                return Err(PeError::BadPeSignature);
            }
            Some(_) => {}
        }

        let coff = pe_offset + 4;
        let machine = r.u16(coff)?;
        let number_of_sections = r.u16(coff + 2)? as usize;
        let size_of_optional_header = r.u16(coff + 16)? as usize;

        let opt = coff + 20;
        match r.u16(opt)? {
            OPT_MAGIC_PE32_PLUS => return Err(PeError::NotPe32),
            OPT_MAGIC_PE32 => {}
            _ => return Err(PeError::NotPe32),
        }
        let entry_rva = r.u32(opt + 16)?;
        let image_base = r.u32(opt + 28)?;
        let size_of_image = r.u32(opt + 56)?;
        let size_of_headers = r.u32(opt + 60)?;
        let number_of_rva_and_dirs = r.u32(opt + 92)?;
        let data_dir = opt + 96;

        if number_of_sections > 96 || size_of_optional_header < 96 {
            return Err(PeError::BadSectionTable);
        }
        let section_table = opt + size_of_optional_header;
        let mut sections = Vec::with_capacity(number_of_sections);
        for i in 0..number_of_sections {
            let base = section_table + i * 40;
            let raw = r.bytes(base, 40).ok_or(PeError::BadSectionTable)?;
            let mut name = [0u8; 8];
            name.copy_from_slice(&raw[0..8]);
            let section = Section {
                name,
                virtual_size: r.u32(base + 8)?,
                virtual_address: r.u32(base + 12)?,
                raw_size: r.u32(base + 16)?,
                raw_pointer: r.u32(base + 20)?,
                characteristics: r.u32(base + 36)?,
            };
            if section.raw_size > 0 {
                let end = (section.raw_pointer as usize).saturating_add(section.raw_size as usize);
                if end > bytes.len() {
                    return Err(PeError::SectionOutOfBounds);
                }
            }
            sections.push(section);
        }

        let mut image = Self {
            machine,
            image_base,
            size_of_image,
            entry_rva,
            sections,
            exports: Vec::new(),
            imports: Vec::new(),
            size_of_headers,
            bytes,
        };

        let directories = number_of_rva_and_dirs.min(16);
        let export_rva = if directories > 0 { r.u32(data_dir)? } else { 0 };
        let import_rva = if directories > 1 { r.u32(data_dir + 8)? } else { 0 };

        if export_rva != 0 {
            image.exports = image.parse_exports(export_rva)?;
        }
        if import_rva != 0 {
            image.imports = image.parse_imports(import_rva)?;
        }
        Ok(image)
    }

    /// File offset for an RVA, if the RVA has file bytes behind it.
    pub fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        if rva < self.size_of_headers {
            let offset = rva as usize;
            return if offset < self.bytes.len() { Some(offset) } else { None };
        }
        for section in &self.sections {
            if section.contains_rva(rva) {
                let delta = rva - section.virtual_address;
                if delta >= section.raw_size {
                    // Zero-filled tail of the section: no file bytes.
                    return None;
                }
                let offset = section.raw_pointer as usize + delta as usize;
                return if offset < self.bytes.len() { Some(offset) } else { None };
            }
        }
        None
    }

    /// First export with the given name.
    pub fn export_by_name(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|e| e.name.as_deref() == Some(name))
    }

    /// First export with the given ordinal.
    pub fn export_by_ordinal(&self, ordinal: u16) -> Option<&Export> {
        self.exports.iter().find(|e| e.ordinal == ordinal)
    }

    fn parse_exports(&self, export_rva: u32) -> Result<Vec<Export>, PeError> {
        let r = Reader { bytes: self.bytes };
        let dir = self.rva_to_offset(export_rva).ok_or(PeError::BadExportDirectory)?;
        let base = r.u32(dir + 16).map_err(|_| PeError::BadExportDirectory)?;
        let number_of_functions = r.u32(dir + 20).map_err(|_| PeError::BadExportDirectory)?;
        let number_of_names = r.u32(dir + 24).map_err(|_| PeError::BadExportDirectory)?;
        let functions_rva = r.u32(dir + 28).map_err(|_| PeError::BadExportDirectory)?;
        let names_rva = r.u32(dir + 32).map_err(|_| PeError::BadExportDirectory)?;
        let ordinals_rva = r.u32(dir + 36).map_err(|_| PeError::BadExportDirectory)?;
        if number_of_names > 65_536 || number_of_functions > 65_536 {
            return Err(PeError::BadExportDirectory);
        }

        let functions = self.rva_to_offset(functions_rva).ok_or(PeError::BadExportDirectory)?;
        let names = self.rva_to_offset(names_rva).ok_or(PeError::BadExportDirectory)?;
        let ordinals = self.rva_to_offset(ordinals_rva).ok_or(PeError::BadExportDirectory)?;

        // index -> name, from the name tables.
        let mut named: Vec<Option<String>> = vec![None; number_of_functions as usize];
        for i in 0..number_of_names {
            let name_rva = r.u32(names + i as usize * 4).map_err(|_| PeError::BadExportDirectory)?;
            let name_offset = self.rva_to_offset(name_rva).ok_or(PeError::BadExportDirectory)?;
            let name = r.cstr(name_offset).map_err(|_| PeError::BadExportDirectory)?;
            let index = r.u16(ordinals + i as usize * 2).map_err(|_| PeError::BadExportDirectory)? as usize;
            let slot = named.get_mut(index).ok_or(PeError::BadExportDirectory)?;
            *slot = Some(name);
        }

        let mut exports = Vec::with_capacity(number_of_functions as usize);
        for index in 0..number_of_functions {
            let rva = r.u32(functions + index as usize * 4).map_err(|_| PeError::BadExportDirectory)?;
            if rva == 0 {
                // Unused export slot.
                continue;
            }
            let ordinal = base.wrapping_add(index);
            exports.push(Export {
                name: named[index as usize].take(),
                ordinal: (ordinal & 0xFFFF) as u16,
                rva,
            });
        }
        Ok(exports)
    }

    fn parse_imports(&self, import_rva: u32) -> Result<Vec<Import>, PeError> {
        let r = Reader { bytes: self.bytes };
        let mut cursor = self.rva_to_offset(import_rva).ok_or(PeError::BadImportDirectory)?;
        let mut imports = Vec::new();
        for _ in 0..4096 {
            let descriptor = r.bytes(cursor, 20).ok_or(PeError::BadImportDirectory)?;
            if descriptor.iter().all(|&b| b == 0) {
                return Ok(imports);
            }
            let original_first_thunk = r.u32(cursor).map_err(|_| PeError::BadImportDirectory)?;
            let name_rva = r.u32(cursor + 12).map_err(|_| PeError::BadImportDirectory)?;
            let first_thunk = r.u32(cursor + 16).map_err(|_| PeError::BadImportDirectory)?;
            let name_offset = self.rva_to_offset(name_rva).ok_or(PeError::BadImportDirectory)?;
            let module = r.cstr(name_offset).map_err(|_| PeError::BadImportDirectory)?;

            let thunks_rva =
                if original_first_thunk != 0 { original_first_thunk } else { first_thunk };
            if thunks_rva != 0 {
                let mut thunk_offset =
                    self.rva_to_offset(thunks_rva).ok_or(PeError::BadImportDirectory)? as usize;
                for _ in 0..4096 {
                    let entry =
                        r.u32(thunk_offset).map_err(|_| PeError::BadImportDirectory)?;
                    if entry == 0 {
                        break;
                    }
                    if entry & 0x8000_0000 != 0 {
                        imports.push(Import {
                            module: module.clone(),
                            name: None,
                            ordinal: Some((entry & 0xFFFF) as u16),
                        });
                    } else {
                        let hint =
                            self.rva_to_offset(entry).ok_or(PeError::BadImportDirectory)? as usize;
                        // Skip the 2-byte hint.
                        let name =
                            r.cstr(hint + 2).map_err(|_| PeError::BadImportDirectory)?;
                        imports.push(Import {
                            module: module.clone(),
                            name: Some(name),
                            ordinal: None,
                        });
                    }
                    thunk_offset += 4;
                }
            }
            cursor += 20;
        }
        Err(PeError::BadImportDirectory)
    }
}

/// Checked little-endian reader.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(&self, offset: usize, len: usize) -> Option<&'a [u8]> {
        let end = offset.checked_add(len)?;
        self.bytes.get(offset..end)
    }

    fn u16(&self, offset: usize) -> Result<u16, PeError> {
        let raw = self.bytes(offset, 2).ok_or(PeError::TooShort)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    fn u32(&self, offset: usize) -> Result<u32, PeError> {
        let raw = self.bytes(offset, 4).ok_or(PeError::TooShort)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    fn cstr(&self, offset: usize) -> Result<String, PeError> {
        let tail = self.bytes.get(offset..).ok_or(PeError::TooShort)?;
        let end = tail.iter().position(|&b| b == 0).ok_or(PeError::TooShort)?;
        Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    //! Byte-exact minimal PE32 fixtures. No donor code is executed to build
    //! them; they are written from the PE/COFF field layout the parser reads.

    /// File offset of the section table in [`minimal_image`] (`opt + 0xE0`).
    pub(crate) const SECTION_TABLE: usize = 0x58 + 0xE0;

    /// One-section PE32 image: `.text` at RVA 0x1000, entry 0x1000,
    /// `ImageBase = 0x00400000`, no data directories.
    pub(crate) fn minimal_image() -> Vec<u8> {
        let mut image = vec![0u8; 0x400];
        image[0..2].copy_from_slice(b"MZ");
        image[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        image[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        image[coff..coff + 2].copy_from_slice(&0x014Cu16.to_le_bytes());
        image[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        image[coff + 16..coff + 18].copy_from_slice(&0xE0u16.to_le_bytes());
        let opt = coff + 20;
        image[opt..opt + 2].copy_from_slice(&0x010Bu16.to_le_bytes());
        image[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes());
        image[opt + 28..opt + 32].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        image[opt + 56..opt + 60].copy_from_slice(&0x2000u32.to_le_bytes());
        image[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
        image[opt + 92..opt + 96].copy_from_slice(&16u32.to_le_bytes());
        let section = SECTION_TABLE;
        image[section..section + 5].copy_from_slice(b".text");
        image[section + 8..section + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        image[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        image[section + 16..section + 20].copy_from_slice(&0x200u32.to_le_bytes());
        image[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes());
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recomp_pe32_rejects_truncated_header() {
        assert_eq!(PeImage::parse(&[]).unwrap_err(), PeError::TooShort);
        assert_eq!(
            PeImage::parse(b"MZ").unwrap_err(),
            PeError::TooShort,
            "a two-byte DOS stub is not an image"
        );
        let mut bytes = vec![0u8; 0x80];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&0x4000u32.to_le_bytes());
        assert_eq!(PeImage::parse(&bytes).unwrap_err(), PeError::TooShort);
    }

    #[test]
    fn recomp_pe32_rejects_bad_magic() {
        let bytes = vec![0u8; 0x80];
        assert_eq!(PeImage::parse(&bytes).unwrap_err(), PeError::BadDosMagic);
        let mut bytes = vec![0u8; 0x100];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        assert_eq!(PeImage::parse(&bytes).unwrap_err(), PeError::BadPeSignature);
    }

    #[test]
    fn recomp_pe32_section_bounds() {
        // Raw section data leaving the buffer must be refused instead of
        // resolving RVAs into bytes that are not there.
        let mut image = tests_support::minimal_image();
        let section = tests_support::SECTION_TABLE;
        image[section + 16..section + 20].copy_from_slice(&0x10_0000u32.to_le_bytes());
        assert_eq!(
            PeImage::parse(&image).unwrap_err(),
            PeError::SectionOutOfBounds
        );
    }

    #[test]
    fn recomp_pe32_parses_entry_and_sections() {
        let image = tests_support::minimal_image();
        let parsed = PeImage::parse(&image).expect("minimal image");
        assert_eq!(parsed.machine, MACHINE_I386);
        assert_eq!(parsed.image_base, 0x0040_0000);
        assert_eq!(parsed.entry_rva, 0x1000);
        assert_eq!(parsed.size_of_image, 0x2000);
        assert_eq!(parsed.sections.len(), 1);
        assert_eq!(parsed.sections[0].name_str(), ".text");
        assert!(parsed.sections[0].contains_rva(0x1000));
        assert!(!parsed.sections[0].contains_rva(0x2000));
        assert_eq!(parsed.rva_to_offset(0x1000), Some(0x200));
    }
}
