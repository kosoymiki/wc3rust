use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let sdl_dir = std::env::var("SDL3_ANDROID_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("third_party/sdl3-android-build"));
    println!("cargo:rerun-if-env-changed=SDL3_ANDROID_LIB");
    println!("cargo:rustc-link-search=native={}", sdl_dir.display());
    println!("cargo:rustc-link-lib=dylib=SDL3");
}
