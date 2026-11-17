use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let prefix = env::var("SDL3_PREFIX")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("third_party/sdl3"));

    println!("cargo:rerun-if-env-changed=SDL3_PREFIX");
    println!(
        "cargo:rerun-if-changed={}",
        prefix.join("lib/libSDL3.a").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=SDL3");
    // SDL3 static library pulls Android platform shims and libc bits.
    for lib in [
        "dl",
        "m",
        "pthread",
        "log",
        "android",
        "GLESv2",
        "GLESv1_CM",
        "EGL",
        "iconv",
        "c++",
    ] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}
