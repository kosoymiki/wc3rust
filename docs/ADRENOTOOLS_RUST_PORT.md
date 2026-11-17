# libadrenotools → Rust: full port

Status: **complete and tested on device** (RMX3709, Android 16, aarch64,
2026-11-12). The project no longer builds or links the donor C/C++; `donors/` keeps
the source only as provenance for the port.

## 1. Donor inventory (byte-level, observed)

`donors/libadrenotools` at `aa39758` (linkernsbypass submodule
`aa3975893d83ef1bc84c321ec60c65fbf1287887`):

| file | lines | role |
|---|---:|---|
| `src/driver.cpp` | 210 | `adrenotools_open_libvulkan`, KGSL import/alloc/map, turbo |
| `src/hook/hook_impl.cpp` | 229 | dlopen hook, sphal hook, fopen hook, GSL hooks |
| `src/hook/main_hook.c` | 8 | exports `android_dlopen_ext`, `android_load_sphal_library` |
| `src/hook/file_redirect_hook.c` | 4 | exports `fopen` |
| `src/hook/gsl_alloc_hook.c` | 13 | exports `gsl_memory_alloc_pure*`, `gsl_memory_free_pure` |
| `lib/linkernsbypass/android_linker_ns.cpp` | 192 | hidden linker symbols, namespaces, unique dlopen |
| `lib/linkernsbypass/elf_soname_patcher.cpp` | 67 | `DT_SONAME` overwrite |
| `src/bcenabler.cpp` | 135 | BCn format-table patch |
| `src/bcenabler_patch.s` + `gen/bcenabler_patch.h` | 23 + 264 B | generated ARM64 patch blob |
| `include/adrenotools/*.h`, `src/hook/kgsl.h` | — | ABI and KGSL UAPI |

## 2. Port matrix (donor file:line → Rust)

| donor | Rust | notes |
|---|---|---|
| `driver.cpp:9-96` | `wc3-adrenotools/src/driver.rs` `open_libvulkan_with_loader` | typed errors; flag validation identical |
| `driver.cpp:100-160` | `kgsl.rs` `import_user_mem`/`alloc_gpu`/`cpu_map`/`free_gpuobj_by_addr` | structs/ioctl numbers pinned by tests |
| `driver.cpp:162-166` | `kgsl.rs` `set_turbo` | same `KGSL_PROP_PWRCTRL` write |
| `hook_impl.cpp:28-36` | `hooks.rs` `set_params`/`init_gsl` | params are a repr(C) block with C strings |
| `hook_impl.cpp:38-140` | `hooks.rs` `hook_dlopen_ext` | same feature order, same fallbacks |
| `hook_impl.cpp:142-158` | `hooks.rs` `hook_load_sphal_library` | sphal/vendor/default |
| `hook_impl.cpp:160-170` | `hooks.rs` `hook_fopen` | `/proc`,`/sys` passthrough; verbatim concat |
| `hook_impl.cpp:172-229` | `hooks.rs` `hook_gsl_alloc_64`/`hook_gsl_free` | same `0xdeadb33f` ownership magic |
| `android_linker_ns.cpp:25-71` | `linker.rs` `load_status`/`create_namespace*`/`namespace_dlopen` | x30 caller capture at wrapper entry |
| `android_linker_ns.cpp:81-122` | `linker.rs` `namespace_dlopen_unique` | memfd ≥ API 29, 2-char soname patch |
| `elf_soname_patcher.cpp:14-66` | `elf.rs` `patch_soname_file` | full bounds checking |
| `bcenabler.cpp:12-20` | `bcn.rs` `get_bcn_type` | same vendor/version rules |
| `bcenabler.cpp:23-135` | `bcn.rs` `patch_bcn` | scans bounded to `/proc/self/maps` regions; generated patch |
| `bcenabler_patch.s` | `bcn.rs` `generate_patch_words` | instruction-encoded from `BCN_FORMAT_TABLE`; byte-identical test |
| `main_hook.c` / `file_redirect_hook.c` / `gsl_alloc_hook.c` | `crates/wc3-adrenotools-{main,fopen,gsl}-hook` | Rust `cdylib`s with the same exports |

## 3. Deliberate deltas (all tested)

1. **No cross-`.so` state sharing.** The donor loads `libhook_impl.so` first so all
   hooks resolve to one copy of `hook_params`. The port gives each hook library its
   own init entry (`adrenotools_main_hook_init`, `adrenotools_fopen_init`,
   `adrenotools_gsl_init_params`) and sets the parameters explicitly. Same effect,
   no link-order contract.
2. **No recursion on `android_dlopen_ext`.** All internal loads go through
   `linker::real_dlopen_ext()`, resolved from `libdl.so`, so an interposed symbol can
   never call itself. The donor relies on dependency-scope ordering.
3. **Bounded ELF parsing and instruction scans.** Every offset is checked against
   the mapped file; every `BL`/`mov w0,wzr` scan is bounded by the containing
   mapping and returns a typed error instead of running off the end.
4. **BCn patch generated in Rust.** The 264-byte blob was replaced by
   `cmp/b.ne/mov/b` encoders driven by a table. The test pins the result to the
   donor's blob, byte for byte.
5. **`MAP_FIXED_NOREPLACE`** for the BCn patch page (falls back to `MAP_FIXED` only
   if the kernel does not know the flag), so a race cannot silently clobber a
   mapping.
6. **I-cache maintenance** (`dc cvau` / `ic ivau` / `dsb ish` / `isb`) instead of
   the donor's bare `ISB`.
7. **Validation**: `hookLibDir` must exist (the donor would later fail inside
   `std::string(nullptr)` or namespace creation); `customDriverDir` + name are
   joined with a `/` if missing for the existence check.
8. **Typed errors** (`DriverError`, `ElfError`, `BcnError`, `EncodeError`) with
   exact reasons; the C ABI still returns `NULL`/`false` like the donor.
9. **Caller address in `create_namespace`** is read from `x30` at wrapper entry
   (`#[inline(never)]` + `asm!("mov {}, x30")`), matching
   `__builtin_return_address(0)`.

## 4. Build mechanics that matter

- The hook libraries must set `DF_1_GLOBAL` (`-z global`): bionic only adds an
  `RTLD_GLOBAL` library to the namespace's preload list when the ELF carries
  `DF_1_GLOBAL`. Termux's cargo config forces `-fuse-ld=mold`, and mold does not
  implement `-z global`, so each hook crate's `build.rs` adds `-fuse-ld=lld` and
  `-Wl,-z,global` for that link. Verified with
  `readelf -d target/debug/libmain_hook.so` → `FLAGS_1 NOW GLOBAL`.
- `-Wl,--exclude-libs,ALL` keeps the rlib's symbols out of the hook exports; the
  only visible symbols are the entry points the loader needs.
- Hook exports after the change:
  - `libmain_hook.so`: `android_dlopen_ext`, `android_load_sphal_library`,
    `adrenotools_main_hook_init`, `adrenotools_hook_load` (probe helper);
  - `libfile_redirect_hook.so`: `fopen`, `adrenotools_fopen_init`;
  - `libgsl_alloc_hook.so`: `gsl_memory_alloc_pure`, `gsl_memory_alloc_pure_64`,
    `gsl_memory_free_pure`, `adrenotools_gsl_init_params`, `adrenotools_gsl_init`.

## 5. Tests

`cargo test -p wc3-adrenotools` → **27 passed**:

- `aarch64`: known donor words (`cmp w0,#131`, `mov w0,#71`, `b.ne +3`, `b +62`),
  BL round-trip, range/alignment rejection, sign extension.
- `bcn`: generated patch **byte-identical** to `gen/bcenabler_patch.h`; type
  classification; size < page.
- `elf`: synthetic ELF soname patch, hostile-offset rejection, and a real
  `/system/lib64/libc.so` copy patched through `mmap` and compared byte for byte.
- `kgsl`: struct sizes/offsets and ioctl numbers (`0x40180932`, `0xc0480936`,
  `0xc0300945`, `0x40200946`, `0xc0300947`, `0xc0200948`) verified against a C
  translation unit built from the donor's `kgsl.h`.
- `hooks`: `"vulkan."` matcher equals the donor's `strstr`; ABI layout; params
  round-trip; no-backend GSL returns `-1`.
- `linker`: `DlextInfo` is 48 bytes; API level readable; **hidden linker symbols
  resolve on device**.
- `proc_maps`: parser and `find_free_page` semantics.

On-device probe (`apps/driver-probe`, exact output 2026-11-12):

```
[1] hidden linker entry points
  PASS
[2] unique /system/lib64/libvulkan.so in a private namespace
  PASS
  libvulkan: Adreno (TM) 730 (api=1.1.128 driver=0x80267064 vendor=0x5143)
[3] custom driver via interposed android_dlopen_ext
  PASS: open_libvulkan returned a libvulkan handle
  PASS: hook loaded the custom driver as the ICD
  custom-icd: Turnip Adreno (TM) 725 (api=1.4.354 driver=0x06802006 vendor=0x5143)
[4] provided Gen8 pack
  observed: driver loaded, but ICD entry points are missing
            (vk_icdNegotiateLoaderICDInterfaceVersion=0x0, vk_icdGetInstanceProcAddr=0x0)
[6b] Turnip via a generated ICD JSON (no hook, plain dlopen loader)
  icd-json: Turnip Adreno (TM) 725 (api=1.4.354 driver=0x06802006 vendor=0x5143)
  PASS: generated ICD JSON drives the bundled loader
result: 0 failure(s)
```

Interpretation:
- the Rust namespace bypass, soname patch, hook, custom driver load and ICD query
  all work;
- the stock AOSP `/system/lib64/libvulkan.so` is **not** a Khronos ICD loader — it
  loads the vendor HAL and enumerates the vendor Adreno blob, but returns 0 devices
  when the hook substitutes Mesa Turnip (it does not use
  `vk_icdGetInstanceProcAddr`). This matches Winlator's architecture, where guest
  Turnip is a rootfs ICD and adrenotools is only used for the host renderer with
  vendor packs. See `docs/WINLATOR_TURNIP_RESEARCH.md`.
- the provided `Turnip_Gen8_V39_Patched.zip` cannot be an ICD: 2154 `.dynsym`
  entries, **0 defined function symbols**, no `vk_icd*` entry points.

## 6. Using it from the game

```rust
// system loader (vendor blobs only)
let opened = unsafe { wc3_adrenotools::driver::open_libvulkan(
    libc::RTLD_NOW,
    wc3_adrenotools::driver::FeatureFlags::CUSTOM,
    std::ptr::null(),            // tmp dir; memfd is used on API >= 29
    hook_dir.as_ptr(),           // nativeLibraryDir with the three hook .so
    driver_dir.as_ptr(),         // installed driver pack directory
    driver_name.as_ptr(),        // e.g. "libvulkan_freedreno.so"
    std::ptr::null(),
    std::ptr::null_mut(),
) }?;
let gipa: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void =
    unsafe { std::mem::transmute(libc::dlsym(opened.vulkan, c"vkGetInstanceProcAddr".as_ptr())) };
```

For Mesa Turnip in a native app, prefer a bundled Khronos loader plus a generated
ICD JSON (`VK_ICD_FILENAMES`), verified by probe test 6b; or call the driver's
`vk_icdGetInstanceProcAddr` directly as probe test 3 does. Do not use the AOSP shim
for Turnip.

## 7. Reproduce

```bash
cd ~/src/herofolends-rs
cargo test -p wc3-adrenotools
cargo build -p wc3-adrenotools-main-hook -p wc3-adrenotools-fopen-hook -p wc3-adrenotools-gsl-hook
./target/debug/driver-probe            # $PREFIX/lib Turnip
readelf -d target/debug/libmain_hook.so | grep FLAGS_1   # NOW GLOBAL
```
