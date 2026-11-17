// Minimal Vulkan ICD probe: create instance, enumerate physical devices, print
// name/apiVersion/driverVersion. No surface/display dependency.
#include <stdlib.h>
#include <vulkan/vulkan.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    VkApplicationInfo app;
    memset(&app, 0, sizeof(app));
    app.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    app.pApplicationName = "wc3rust-vk-probe";
    app.applicationVersion = 1;
    app.apiVersion = VK_API_VERSION_1_4;

    VkInstanceCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    ci.pApplicationInfo = &app;

    VkInstance inst = VK_NULL_HANDLE;
    VkResult r = vkCreateInstance(&ci, NULL, &inst);
    if (r != VK_SUCCESS) {
        printf("vkCreateInstance failed: %d\n", (int)r);
        return 1;
    }
    uint32_t n = 0;
    r = vkEnumeratePhysicalDevices(inst, &n, NULL);
    if (r != VK_SUCCESS) {
        printf("vkEnumeratePhysicalDevices(count) failed: %d\n", (int)r);
        vkDestroyInstance(inst, NULL);
        return 1;
    }
    printf("physical_devices=%u\n", n);
    if (n > 0) {
        VkPhysicalDevice *devs = (VkPhysicalDevice *)calloc(n, sizeof(*devs));
        r = vkEnumeratePhysicalDevices(inst, &n, devs);
        if (r == VK_SUCCESS) {
            for (uint32_t i = 0; i < n; ++i) {
                VkPhysicalDeviceProperties p;
                vkGetPhysicalDeviceProperties(devs[i], &p);
                printf("GPU%u name=%s api=%u.%u.%u driver=%u type=%u\n", i, p.deviceName,
                       VK_VERSION_MAJOR(p.apiVersion), VK_VERSION_MINOR(p.apiVersion),
                       VK_VERSION_PATCH(p.apiVersion), p.driverVersion, (unsigned)p.deviceType);
            }
        }
        free(devs);
    }
    vkDestroyInstance(inst, NULL);
    return n > 0 ? 0 : 2;
}
