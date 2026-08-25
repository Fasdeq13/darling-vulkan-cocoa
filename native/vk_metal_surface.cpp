#include <vulkan/vulkan.h>
#include <vulkan/vulkan_metal.h>
#include <cstdint>
#include <cstring>

extern "C" {

struct QmvSurfaceResult {
    VkSurfaceKHR surface;
    int32_t result_code;
};
QmvSurfaceResult qmv_create_metal_surface(VkInstance instance, const void* metal_layer) {
    QmvSurfaceResult out{};
    out.surface = VK_NULL_HANDLE;

    if (instance == VK_NULL_HANDLE || metal_layer == nullptr) {
        out.result_code = VK_ERROR_INITIALIZATION_FAILED;
        return out;
    }

    VkMetalSurfaceCreateInfoEXT create_info{};
    create_info.sType = VK_STRUCTURE_TYPE_METAL_SURFACE_CREATE_INFO_EXT;
    create_info.pNext = nullptr;
    create_info.flags = 0;
    create_info.pLayer = static_cast<const CAMetalLayer*>(metal_layer);

    auto create_fn = reinterpret_cast<PFN_vkCreateMetalSurfaceEXT>(
        vkGetInstanceProcAddr(instance, "vkCreateMetalSurfaceEXT"));

    if (create_fn == nullptr) {
        out.result_code = VK_ERROR_EXTENSION_NOT_PRESENT;
        return out;
    }

    VkResult result = create_fn(instance, &create_info, nullptr, &out.surface);
    out.result_code = static_cast<int32_t>(result);
    return out;
}

void qmv_destroy_metal_surface(VkInstance instance, VkSurfaceKHR surface) {
    if (instance == VK_NULL_HANDLE || surface == VK_NULL_HANDLE) {
        return;
    }
    auto destroy_fn = reinterpret_cast<PFN_vkDestroySurfaceKHR>(
        vkGetInstanceProcAddr(instance, "vkDestroySurfaceKHR"));
    if (destroy_fn != nullptr) {
        destroy_fn(instance, surface, nullptr);
    }
}

int32_t qmv_get_surface_capabilities(VkInstance instance, VkPhysicalDevice physical_device,
                                       VkSurfaceKHR surface, uint32_t* out_min_image_count,
                                       uint32_t* out_max_image_count, uint32_t* out_width,
                                       uint32_t* out_height) {
    if (instance == VK_NULL_HANDLE || physical_device == VK_NULL_HANDLE || surface == VK_NULL_HANDLE) {
        return VK_ERROR_INITIALIZATION_FAILED;
    }

    auto query_fn = reinterpret_cast<PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR>(
        vkGetInstanceProcAddr(instance, "vkGetPhysicalDeviceSurfaceCapabilitiesKHR"));

    if (query_fn == nullptr) {
        return VK_ERROR_EXTENSION_NOT_PRESENT;
    }

    VkSurfaceCapabilitiesKHR caps{};
    VkResult result = query_fn(physical_device, surface, &caps);
    if (result != VK_SUCCESS) {
        return static_cast<int32_t>(result);
    }

    if (out_min_image_count) {
        *out_min_image_count = caps.minImageCount;
    }
    if (out_max_image_count) {
        *out_max_image_count = caps.maxImageCount;
    }
    if (out_width) {
        *out_width = caps.currentExtent.width;
    }
    if (out_height) {
        *out_height = caps.currentExtent.height;
    }

    return VK_SUCCESS;
}

}
