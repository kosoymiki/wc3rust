fn main() {
    // SDL loads this file by path, but a matching SONAME makes a preload
    // (if one is used) resolve to the same object.
    println!("cargo:rustc-link-arg=-Wl,-soname,libvulkan.so");
}
