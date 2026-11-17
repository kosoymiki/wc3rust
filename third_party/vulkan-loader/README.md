# Vendored Khronos Vulkan loader (Android/bionic, aarch64)

`libvulkan.so` is the Khronos Vulkan loader 1.4.368 built for Android/bionic,
taken from the Termux `vulkan-loader` package (`$PREFIX/lib/libvulkan.so.1.4.368`,
SHA-256 recorded below). It is Apache-2.0; see `LICENSE`.

Unlike the AOSP loader in `/system/lib64/libvulkan.so`, this loader honours
`VK_ICD_FILENAMES`/`VK_DRIVER_FILES`, which lets the app load a user-provided
Mesa Turnip ICD without replacing the system driver. The app falls back to the
vendor driver when no Turnip ICD is installed.

Source package: `vulkan-loader` (Termux), loader 1.4.368.
13d6b4eb5ea3ae6870425d8fe27f99ca88c21e2272cb7b23c94ea5caeca8cf4e  third_party/vulkan-loader/libvulkan.so
