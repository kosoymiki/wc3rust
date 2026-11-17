fn main() {
    // `-z global` sets DF_1_GLOBAL so RTLD_GLOBAL adds this library to the
    // namespace's LD_PRELOAD list (the mechanism the donor relies on).
    // Force lld for this link: mold does not implement `-z global`, which sets
    // DF_1_GLOBAL (required for the RTLD_GLOBAL preload-list behaviour).
    println!("cargo:rustc-cdylib-link-arg=-fuse-ld=lld");
    println!("cargo:rustc-cdylib-link-arg=-Wl,-z,global");
    println!("cargo:rustc-cdylib-link-arg=-Wl,--as-needed");
    // Hide everything pulled out of the static rlib; only this crate's own
    // #[no_mangle] entry points stay visible in the hook namespace.
    println!("cargo:rustc-cdylib-link-arg=-Wl,--exclude-libs,ALL");
}
