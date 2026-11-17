fn main() {
    // LD_PRELOAD must satisfy both SDL's and wgpu/ash's dlopen("libvulkan.so").
    println!("cargo:rustc-link-arg=-Wl,-soname,libvulkan.so");
}
