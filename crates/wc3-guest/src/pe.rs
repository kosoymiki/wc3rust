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
//!
//! # Resource bounds
//!
//! The import table is the one place where a small file can describe a large
//! structure: several descriptors may point at one shared thunk array, so the
//! descriptor and thunk loops cannot be bounded independently
//! (audit `L547-01`: 99 KB → 1.39 GiB before this cap). Three rules keep the
//! parse proportional to the input:
//!
//! - [`MAX_IMPORT_DESCRIPTORS`] descriptors, [`MAX_THUNKS_PER_DESCRIPTOR`]
//!   entries each, and a hard [`MAX_IMPORTS`] on the *flattened* result — the
//!   parse returns [`PeError::BadImportDirectory`] once the cap is reached
//!   instead of growing without bound;
//! - a thunk array must contain its `0` terminator within the per-descriptor
//!   window; running out is a typed error, never a silent truncation;
//! - a module name is parsed once per name RVA and shared ([`Arc<str>`]) by
//!   every descriptor and symbol that uses it, so a repeated name is one
//!   allocation, not one per symbol;
//! - every name read is bounded ([`MAX_NAME_LEN`]) and all names in one image
//!   share a [`MAX_TOTAL_NAME_BYTES`] budget, so tens of thousands of pointers
//!   into one long string cannot multiply into gigabytes; an import-name RVA
//!   seen twice is parsed once and shared.
//!
//! Export tables are structurally validated before allocation: the function,
//! name and ordinal tables must lie entirely inside the file, so a header that
//! claims 65 536 exports in a 200-byte file is rejected before a vector is
//! sized from it.
//!
//! # Forwarded exports
//!
//! An export whose RVA points inside the export directory is a *forwarder*
//! (`KERNEL32.Sleep`), not code. [`Export::forwarder`] records the string, so a
//! consumer that maps exports to functions never treats a forwarder RVA as an
//! address.

use std::sync::Arc;

/// `IMAGE_FILE_MACHINE_I386`.
pub const MACHINE_I386: u16 = 0x014C;
/// `IMAGE_NT_OPTIONAL_HDR32_MAGIC`.
pub const OPT_MAGIC_PE32: u16 = 0x010B;
/// `IMAGE_NT_OPTIONAL_HDR64_MAGIC` (rejected).
pub const OPT_MAGIC_PE32_PLUS: u16 = 0x020B;

/// Maximum section count the PE/COFF format allows.
pub const MAX_SECTIONS: usize = 96;
/// Maximum export slots (`NumberOfFunctions`) accepted.
pub const MAX_EXPORTS: u32 = 65_536;
/// Maximum flattened import entries accepted (see module docs).
pub const MAX_IMPORTS: usize = 65_536;
/// Maximum import descriptors walked before the table must terminate.
pub const MAX_IMPORT_DESCRIPTORS: usize = 4096;
/// Maximum thunk entries walked per descriptor before it must terminate.
pub const MAX_THUNKS_PER_DESCRIPTOR: usize = 4096;
/// Maximum length of one module, symbol or forwarder name.
pub const MAX_NAME_LEN: usize = 4096;
/// Maximum bytes of all names (modules, symbols, forwarders) in one image.
pub const MAX_TOTAL_NAME_BYTES: usize = 4 * 1024 * 1024;

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
    /// Optional header is smaller than the fields the parser reads.
    BadOptionalHeader,
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
    /// Virtual size.
    pub virtual_size: u32,
    /// RVA of the section.
    pub virtual_address: u32,
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

    /// `IMAGE_SCN_MEM_EXECUTE` (`0x2000_0000`).
    pub fn is_executable(&self) -> bool {
        self.characteristics & 0x2000_0000 != 0
    }
}

/// One exported symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// Name when the export is named, `None` for ordinal-only exports.
    pub name: Option<String>,
    /// Ordinal (export directory base + index).
    pub ordinal: u16,
    /// RVA of the export (for a forwarder: the RVA of the forwarder string).
    pub rva: u32,
    /// `Some("DLL.Symbol")` when the export is forwarded instead of code.
    pub forwarder: Option<String>,
}

impl Export {
    /// True when this export forwards to another module.
    pub fn is_forwarder(&self) -> bool {
        self.forwarder.is_some()
    }
}

/// One imported symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// Guest DLL name as written in the import descriptor, shared by every
    /// symbol of that descriptor.
    pub module: Arc<str>,
    /// Import name, `None` when imported by ordinal.
    pub name: Option<Arc<str>>,
    /// Import ordinal when imported by ordinal.
    pub ordinal: Option<u16>,
}

impl Import {
    /// The module name as a string slice.
    pub fn module_str(&self) -> &str {
        &self.module
    }

    /// The import name as a string slice (`None` for an ordinal import).
    pub fn name_str(&self) -> Option<&str> {
        self.name.as_deref()
    }
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
    /// Export directory entries, ordered by export index (a function with
    /// several alias names appears once per name).
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

        let coff = pe_offset.checked_add(4).ok_or(PeError::TooShort)?;
        let machine = r.u16(coff)?;
        let number_of_sections = r.u16(coff + 2)? as usize;
        let size_of_optional_header = r.u16(coff + 16)? as usize;

        let opt = coff.checked_add(20).ok_or(PeError::TooShort)?;
        match r.u16(opt)? {
            OPT_MAGIC_PE32_PLUS => return Err(PeError::NotPe32),
            OPT_MAGIC_PE32 => {}
            _ => return Err(PeError::NotPe32),
        }
        if size_of_optional_header < 96 {
            return Err(PeError::BadOptionalHeader);
        }
        let entry_rva = r.u32(opt + 16)?;
        let image_base = r.u32(opt + 28)?;
        let size_of_image = r.u32(opt + 56)?;
        let size_of_headers = r.u32(opt + 60)?;
        let number_of_rva_and_dirs = r.u32(opt + 92)?;

        if number_of_sections > MAX_SECTIONS {
            return Err(PeError::BadSectionTable);
        }
        let section_table = opt.checked_add(size_of_optional_header).ok_or(PeError::BadSectionTable)?;
        let table_bytes = number_of_sections
            .checked_mul(40)
            .ok_or(PeError::BadSectionTable)?;
        if r.bytes(section_table, table_bytes).is_none() {
            return Err(PeError::BadSectionTable);
        }
        let mut sections = Vec::with_capacity(number_of_sections);
        for i in 0..number_of_sections {
            let base = section_table + i * 40;
            // The whole table was bounds-checked above; these reads cannot fail.
            let raw = r.bytes(base, 40).ok_or(PeError::BadSectionTable)?;
            let mut name = [0u8; 8];
            name.copy_from_slice(&raw[0..8]);
            let section = Section {
                name,
                virtual_size: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
                virtual_address: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
                raw_size: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
                raw_pointer: u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]),
                characteristics: u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]),
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

        // The data-directory array lives inside the optional header: a header
        // that declares fewer bytes than `NumberOfRvaAndSizes` claims must not
        // make the parser read section headers (or anything else) as
        // directories.
        let dir_space = (size_of_optional_header - 96) / 8;
        let directories = number_of_rva_and_dirs.min(16).min(dir_space as u32);
        let data_dir = opt.checked_add(96).ok_or(PeError::BadOptionalHeader)?;
        let export_rva = if directories > 0 { r.u32(data_dir)? } else { 0 };
        let export_size = if directories > 0 { r.u32(data_dir + 4)? } else { 0 };
        let import_rva = if directories > 1 { r.u32(data_dir + 8)? } else { 0 };

        if export_rva != 0 {
            image.exports = image.parse_exports(export_rva, export_size)?;
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

    /// The section that covers `rva`, if any.
    pub fn section_for_rva(&self, rva: u32) -> Option<&Section> {
        self.sections.iter().find(|section| section.contains_rva(rva))
    }

    /// First export with the given name.
    pub fn export_by_name(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|e| e.name.as_deref() == Some(name))
    }

    /// First export with the given ordinal.
    pub fn export_by_ordinal(&self, ordinal: u16) -> Option<&Export> {
        self.exports.iter().find(|e| e.ordinal == ordinal)
    }

    /// Import modules in first-appearance order, without duplicates.
    pub fn import_modules(&self) -> Vec<&str> {
        let mut modules: Vec<&str> = Vec::new();
        for import in &self.imports {
            let name = import.module_str();
            if !modules.contains(&name) {
                modules.push(name);
            }
        }
        modules
    }

    fn parse_exports(&self, export_rva: u32, export_size: u32) -> Result<Vec<Export>, PeError> {
        let r = Reader { bytes: self.bytes };
        let dir = self.rva_to_offset(export_rva).ok_or(PeError::BadExportDirectory)?;
        let base = r.u32(dir + 16).map_err(|_| PeError::BadExportDirectory)?;
        let number_of_functions = r.u32(dir + 20).map_err(|_| PeError::BadExportDirectory)?;
        let number_of_names = r.u32(dir + 24).map_err(|_| PeError::BadExportDirectory)?;
        let functions_rva = r.u32(dir + 28).map_err(|_| PeError::BadExportDirectory)?;
        let names_rva = r.u32(dir + 32).map_err(|_| PeError::BadExportDirectory)?;
        let ordinals_rva = r.u32(dir + 36).map_err(|_| PeError::BadExportDirectory)?;
        if number_of_functions > MAX_EXPORTS || number_of_names > MAX_EXPORTS {
            return Err(PeError::BadExportDirectory);
        }
        // A count with no table is a malformed directory; without this check a
        // zero RVA resolves to the DOS header and is read as a table.
        if number_of_functions > 0 && functions_rva == 0 {
            return Err(PeError::BadExportDirectory);
        }
        if number_of_names > 0 && (names_rva == 0 || ordinals_rva == 0) {
            return Err(PeError::BadExportDirectory);
        }

        let functions = self.rva_to_offset(functions_rva).ok_or(PeError::BadExportDirectory)?;
        // Validate the whole table before allocating or looping: a tiny file
        // that claims 65 536 functions must be rejected structurally.
        let functions_bytes = (number_of_functions as usize)
            .checked_mul(4)
            .ok_or(PeError::BadExportDirectory)?;
        if functions.checked_add(functions_bytes).map_or(true, |end| end > self.bytes.len()) {
            return Err(PeError::BadExportDirectory);
        }
        let (names, ordinals) = if number_of_names > 0 {
            let names = self.rva_to_offset(names_rva).ok_or(PeError::BadExportDirectory)?;
            let ordinals = self.rva_to_offset(ordinals_rva).ok_or(PeError::BadExportDirectory)?;
            let names_bytes = (number_of_names as usize)
                .checked_mul(4)
                .ok_or(PeError::BadExportDirectory)?;
            let ordinals_bytes = (number_of_names as usize)
                .checked_mul(2)
                .ok_or(PeError::BadExportDirectory)?;
            if names.checked_add(names_bytes).map_or(true, |end| end > self.bytes.len())
                || ordinals
                    .checked_add(ordinals_bytes)
                    .map_or(true, |end| end > self.bytes.len())
            {
                return Err(PeError::BadExportDirectory);
            }
            (names, ordinals)
        } else {
            (0, 0)
        };

        // index -> names, from the name tables. A function may have several
        // names (aliases); each alias becomes its own export record.
        let mut names_budget = NameBudget::new();
        let mut named: Vec<Vec<String>> = vec![Vec::new(); number_of_functions as usize];
        for i in 0..number_of_names {
            let name_rva = r.u32(names + i as usize * 4).map_err(|_| PeError::BadExportDirectory)?;
            if name_rva == 0 {
                return Err(PeError::BadExportDirectory);
            }
            let name_offset = self.rva_to_offset(name_rva).ok_or(PeError::BadExportDirectory)?;
            let name = r
                .cstr_bounded(name_offset, MAX_NAME_LEN)
                .map_err(|_| PeError::BadExportDirectory)?;
            if !names_budget.take(name.len()) {
                return Err(PeError::BadExportDirectory);
            }
            let index = r.u16(ordinals + i as usize * 2).map_err(|_| PeError::BadExportDirectory)? as usize;
            let slot = named.get_mut(index).ok_or(PeError::BadExportDirectory)?;
            slot.push(name);
        }

        // Anything pointing into the export directory is a forwarder string,
        // not a code RVA; the directory size is zero in images that omit it.
        let forwarder_end = export_rva.saturating_add(export_size);
        let mut exports = Vec::with_capacity(number_of_functions as usize);
        for index in 0..number_of_functions {
            let rva = r.u32(functions + index as usize * 4).map_err(|_| PeError::BadExportDirectory)?;
            let names = core::mem::take(&mut named[index as usize]);
            if rva == 0 {
                if !names.is_empty() {
                    // A name that refers to an unused export slot is malformed.
                    return Err(PeError::BadExportDirectory);
                }
                // Unused export slot.
                continue;
            }
            let ordinal = base as u64 + index as u64;
            let ordinal = u16::try_from(ordinal).map_err(|_| PeError::BadExportDirectory)?;
            let forwarder = if export_size != 0 && rva >= export_rva && rva < forwarder_end {
                let offset = self.rva_to_offset(rva).ok_or(PeError::BadExportDirectory)?;
                let target = r
                    .cstr_bounded(offset, MAX_NAME_LEN)
                    .map_err(|_| PeError::BadExportDirectory)?;
                if !names_budget.take(target.len()) {
                    return Err(PeError::BadExportDirectory);
                }
                Some(target)
            } else {
                None
            };
            if names.is_empty() {
                exports.push(Export { name: None, ordinal, rva, forwarder });
            } else {
                for name in names {
                    exports.push(Export {
                        name: Some(name),
                        ordinal,
                        rva,
                        forwarder: forwarder.clone(),
                    });
                }
            }
        }
        Ok(exports)
    }

    fn parse_imports(&self, import_rva: u32) -> Result<Vec<Import>, PeError> {
        let r = Reader { bytes: self.bytes };
        let mut cursor = self.rva_to_offset(import_rva).ok_or(PeError::BadImportDirectory)?;
        let mut imports = Vec::new();
        let mut descriptors = 0usize;
        let mut budget = NameBudget::new();
        // Import-name RVA -> parsed name. Several thunks routinely share one
        // hint/name entry; parse it once and share the `Arc<str>` (BTreeMap, not
        // a linear scan: 65 536 distinct RVAs would make the scan O(n²)).
        let mut name_cache: std::collections::BTreeMap<u32, Arc<str>> = std::collections::BTreeMap::new();
        // Module name RVA -> shared module name. Descriptors may repeat one
        // module (and a hostile image can repeat it thousands of times).
        let mut module_cache: std::collections::BTreeMap<u32, Arc<str>> =
            std::collections::BTreeMap::new();
        loop {
            let descriptor = r.bytes(cursor, 20).ok_or(PeError::BadImportDirectory)?;
            if descriptor.iter().all(|&b| b == 0) {
                return Ok(imports);
            }
            if descriptors == MAX_IMPORT_DESCRIPTORS {
                return Err(PeError::BadImportDirectory);
            }
            descriptors += 1;
            let original_first_thunk = r.u32(cursor).map_err(|_| PeError::BadImportDirectory)?;
            let name_rva = r.u32(cursor + 12).map_err(|_| PeError::BadImportDirectory)?;
            let first_thunk = r.u32(cursor + 16).map_err(|_| PeError::BadImportDirectory)?;
            if name_rva == 0 {
                return Err(PeError::BadImportDirectory);
            }
            let name_offset = self.rva_to_offset(name_rva).ok_or(PeError::BadImportDirectory)?;
            let module: Arc<str> = match module_cache.get(&name_rva) {
                Some(cached) => Arc::clone(cached),
                None => {
                    let module_name = r
                        .cstr_bounded(name_offset, MAX_NAME_LEN)
                        .map_err(|_| PeError::BadImportDirectory)?;
                    if module_name.is_empty() || !budget.take(module_name.len()) {
                        return Err(PeError::BadImportDirectory);
                    }
                    let parsed: Arc<str> = Arc::from(module_name);
                    module_cache.insert(name_rva, Arc::clone(&parsed));
                    parsed
                }
            };

            let thunks_rva =
                if original_first_thunk != 0 { original_first_thunk } else { first_thunk };
            if thunks_rva == 0 {
                return Err(PeError::BadImportDirectory);
            }
            let mut thunk_offset =
                self.rva_to_offset(thunks_rva).ok_or(PeError::BadImportDirectory)?;
            let mut terminated = false;
            for _ in 0..MAX_THUNKS_PER_DESCRIPTOR {
                let entry = r.u32(thunk_offset).map_err(|_| PeError::BadImportDirectory)?;
                if entry == 0 {
                    terminated = true;
                    break;
                }
                if imports.len() >= MAX_IMPORTS {
                    return Err(PeError::BadImportDirectory);
                }
                if entry & 0x8000_0000 != 0 {
                    imports.push(Import {
                        module: Arc::clone(&module),
                        name: None,
                        ordinal: Some((entry & 0xFFFF) as u16),
                    });
                } else {
                    let name = match name_cache.get(&entry) {
                        Some(cached) => Arc::clone(cached),
                        None => {
                            let hint =
                                self.rva_to_offset(entry).ok_or(PeError::BadImportDirectory)? as usize;
                            // Skip the 2-byte hint.
                            let raw = r
                                .cstr_bounded(hint + 2, MAX_NAME_LEN)
                                .map_err(|_| PeError::BadImportDirectory)?;
                            if raw.is_empty() || !budget.take(raw.len()) {
                                return Err(PeError::BadImportDirectory);
                            }
                            let parsed: Arc<str> = Arc::from(raw);
                            name_cache.insert(entry, Arc::clone(&parsed));
                            parsed
                        }
                    };
                    imports.push(Import { module: Arc::clone(&module), name: Some(name), ordinal: None });
                }
                thunk_offset += 4;
            }
            if !terminated {
                // A declared thunk array that never reaches its terminator is
                // malformed; silently stopping would under-report the imports
                // the image actually asks the loader to bind.
                return Err(PeError::BadImportDirectory);
            }
            cursor += 20;
        }
    }
}

/// Checked little-endian reader.
struct Reader<'a> {
    bytes: &'a [u8],
}

/// Total-bytes budget for the names of one image.
///
/// Table sizes are bounded by [`MAX_EXPORTS`]/[`MAX_IMPORTS`]; names are not
/// part of those tables, so a single long NUL-terminated string can be pointed
/// at from thousands of entries. The budget makes the string cost of a parse
/// independent of how many entries share a string.
struct NameBudget {
    used: usize,
}

impl NameBudget {
    fn new() -> Self {
        Self { used: 0 }
    }

    /// Account for `len` bytes; false when the image exceeds the budget.
    fn take(&mut self, len: usize) -> bool {
        self.used = self.used.saturating_add(len);
        self.used <= MAX_TOTAL_NAME_BYTES
    }
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

    /// NUL-terminated string, scanning at most `max_len` bytes. A string
    /// without a terminator inside the window is malformed, not "whatever is
    /// left in the file".
    fn cstr_bounded(&self, offset: usize, max_len: usize) -> Result<String, PeError> {
        let tail = self.bytes.get(offset..).ok_or(PeError::TooShort)?;
        let window = &tail[..tail.len().min(max_len)];
        let end = window.iter().position(|&b| b == 0).ok_or(PeError::TooShort)?;
        Ok(String::from_utf8_lossy(&window[..end]).into_owned())
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
        assert_eq!(parsed.section_for_rva(0x1100).map(Section::name_str), Some(".text".into()));
        assert!(parsed.section_for_rva(0x9000).is_none());
    }

    #[test]
    fn recomp_pe32_short_optional_header_is_typed() {
        let mut image = tests_support::minimal_image();
        let coff = 0x44;
        image[coff + 16..coff + 18].copy_from_slice(&0x40u16.to_le_bytes());
        assert_eq!(
            PeImage::parse(&image).unwrap_err(),
            PeError::BadOptionalHeader,
            "a 0x40-byte optional header cannot hold the fields the parser reads"
        );
    }

    #[test]
    fn recomp_pe32_data_directories_cannot_overrun_optional_header() {
        // NumberOfRvaAndSizes claims 16 directories but the optional header is
        // only 96 bytes long: the directory array is empty, so the words at
        // opt+96 belong to the section table and must not be read as RVAs.
        let mut image = tests_support::minimal_image();
        let coff = 0x44;
        image[coff + 16..coff + 18].copy_from_slice(&96u16.to_le_bytes());
        // opt+96 == section table start: the section name ".text" would be a
        // nonsense export RVA if the clamp were missing.
        let parsed = PeImage::parse(&image).expect("no directories are readable");
        assert!(parsed.exports.is_empty());
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn recomp_pe32_export_count_without_table_is_typed() {
        let mut image = tests_support::minimal_image();
        let opt = 0x58;
        image[opt + 96..opt + 100].copy_from_slice(&0x1000u32.to_le_bytes());
        image[opt + 104..opt + 108].copy_from_slice(&0x2000u32.to_le_bytes());
        let dir = 0x200usize;
        // Export directory: 1 function, but AddressOfFunctions is 0.
        image[dir + 16..dir + 20].copy_from_slice(&1u32.to_le_bytes());
        image[dir + 20..dir + 24].copy_from_slice(&1u32.to_le_bytes());
        image[dir + 24..dir + 28].copy_from_slice(&0u32.to_le_bytes());
        image[dir + 28..dir + 32].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            PeImage::parse(&image).unwrap_err(),
            PeError::BadExportDirectory
        );
    }

    #[test]
    fn recomp_pe32_export_table_beyond_file_is_typed() {
        // 65 536 functions starting at offset 0x400 in a 0x400-byte image: the
        // table cannot fit, so the parse must fail before allocating it.
        let mut image = tests_support::minimal_image();
        let opt = 0x58;
        image[opt + 96..opt + 100].copy_from_slice(&0x1000u32.to_le_bytes());
        image[opt + 104..opt + 108].copy_from_slice(&0x2000u32.to_le_bytes());
        let dir = 0x200usize;
        image[dir + 16..dir + 20].copy_from_slice(&1u32.to_le_bytes());
        image[dir + 20..dir + 24].copy_from_slice(&65_536u32.to_le_bytes());
        image[dir + 24..dir + 28].copy_from_slice(&0u32.to_le_bytes());
        image[dir + 28..dir + 32].copy_from_slice(&0x1000u32.to_le_bytes());
        assert_eq!(
            PeImage::parse(&image).unwrap_err(),
            PeError::BadExportDirectory
        );
    }
}
