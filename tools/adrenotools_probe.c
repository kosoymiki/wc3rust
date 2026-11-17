// libadrenotools probe: load a custom Vulkan driver through the rootless hook.
// Build (Termux/Android):
//   cc -I donors/libadrenotools/include tools/adrenotools_probe.c \
//      -L third_party/adrenotools-build -ladrenotools -ldl -llog -landroid -o tools/adrenotools_probe
#include <adrenotools/driver.h>
#include <stdio.h>
#include <string.h>
#include <dlfcn.h>

int main(int argc, char **argv) {
    const char *custom_dir = argc > 1 ? argv[1] : "/data/data/com.termux/files/usr/lib";
    const char *custom_name = argc > 2 ? argv[2] : "libvulkan_freedreno.so";
    const char *hook_dir = argc > 3 ? argv[3] : "third_party/adrenotools-build/src/hook";
    const char *tmp_dir = argc > 4 ? argv[4] : "/data/data/com.termux/files/usr/tmp";

    printf("adrenotools custom_dir=%s name=%s hook_dir=%s\n", custom_dir, custom_name, hook_dir);
    void *handle = adrenotools_open_libvulkan(RTLD_NOW, ADRENOTOOLS_DRIVER_CUSTOM,
                                              tmp_dir, hook_dir, custom_dir,
                                              custom_name, NULL, NULL);
    if (!handle) {
        printf("adrenotools_open_libvulkan = NULL\n");
        return 1;
    }
    printf("adrenotools_open_libvulkan OK handle=%p\n", handle);
    void *(*gpa)(void *, const char *) = (void *(*)(void *, const char *))dlsym(handle, "vkGetInstanceProcAddr");
    if (!gpa) {
        printf("vkGetInstanceProcAddr missing: %s\n", dlerror());
        return 1;
    }
    printf("vkGetInstanceProcAddr OK\n");
    void *create = gpa(NULL, "vkCreateInstance");
    printf("vkCreateInstance=%p\n", create);
    return create ? 0 : 1;
}
