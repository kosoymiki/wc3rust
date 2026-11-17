//! Clean-room Rust port of [libadrenotools](https://github.com/bylaws/libadrenotools).
//!
//! The C++ donor consists of four moving parts, all re-implemented here from the
//! donor's observable behaviour (file:line references in
//! `docs/ADRENOTOOLS_RUST_PORT.md`):
//!
//! 1. [`linker`] — Android linker-namespace bypass (`android_linker_ns.cpp`):
//!    resolves the hidden `__loader_android_*` entry points through `ld-android.so`
//!    and `libdl_android.so`, creates namespaces and opens libraries inside them.
//! 2. [`elf`] — `DT_SONAME` patching (`elf_soname_patcher.cpp`), reimplemented with
//!    full bounds checking instead of raw pointer arithmetic.
//! 3. [`hooks`] — the interposition layer (`hook_impl.cpp`): `android_dlopen_ext`,
//!    `android_load_sphal_library`, `fopen`, `gsl_memory_alloc_pure*`.
//! 4. [`driver`] — `adrenotools_open_libvulkan` plus the KGSL mapping helpers
//!    (`driver.cpp`), and [`bcn`] — the BCn enabler (`bcenabler.cpp`) whose ARM64
//!    patch is generated in Rust instead of being emitted by a C toolchain.
//!
//! The hook entry points live in three `cdylib` crates so the same mechanism as the
//! donor is preserved: `-z global` + `RTLD_GLOBAL` puts them on the namespace's
//! preload list, which is what lets the Vulkan loader's `android_dlopen_ext` call
//! reach [`hooks::hook_dlopen_ext`].
//!
//! Everything is Android/aarch64 specific: the crate compiles for
//! `aarch64-linux-android` (Termux host triple) and is intended to run inside an
//! Android app process.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod aarch64;
pub mod bcn;
pub mod driver;
pub mod elf;
pub mod hooks;
pub mod kgsl;
pub mod linker;
pub mod proc_maps;

pub use bcn::BcnType;
pub use driver::{DriverError, FeatureFlags, OpenedDriver};
pub use hooks::{GpuMapping, HookParams};
