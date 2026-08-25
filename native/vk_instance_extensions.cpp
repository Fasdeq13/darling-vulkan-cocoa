#include <vulkan/vulkan.h>
#include <cstdint>
#include <cstring>
#include <vector>
#include <string>

extern "C" {

struct QmvExtensionCheckResult {
    int32_t has_metal_surface;
    int32_t has_portability_enumeration;
    int32_t has_external_memory_capabilities;
    int32_t has_debug_utils;
    uint32_t total_extension_count;
};

QmvExtensionCheckResult qmv_check_darwin_instance_extensions() {
    QmvExtensionCheckResult result{};

    uint32_t extension_count = 0;
    VkResult enum_result = vkEnumerateInstanceExtensionProperties(nullptr, &extension_count, nullptr);
    if (enum_result != VK_SUCCESS || extension_count == 0) {
        return result;
    }

    std::vector<VkExtensionProperties> extensions(extension_count);
    enum_result = vkEnumerateInstanceExtensionProperties(nullptr, &extension_count, extensions.data());
    if (enum_result != VK_SUCCESS) {
        return result;
    }

    result.total_extension_count = extension_count;

    for (const auto& ext : extensions) {
        if (std::strcmp(ext.extensionName, "VK_EXT_metal_surface") == 0) {
            result.has_metal_surface = 1;
        } else if (std::strcmp(ext.extensionName, "VK_KHR_portability_enumeration") == 0) {
            result.has_portability_enumeration = 1;
        } else if (std::strcmp(ext.extensionName, "VK_KHR_external_memory_capabilities") == 0) {
            result.has_external_memory_capabilities = 1;
        } else if (std::strcmp(ext.extensionName, "VK_EXT_debug_utils") == 0) {
            result.has_debug_utils = 1;
        }
    }

    return result;
}

int32_t qmv_is_extension_available(const char* extension_name) {
    if (extension_name == nullptr) {
        return 0;
    }

    uint32_t extension_count = 0;
    if (vkEnumerateInstanceExtensionProperties(nullptr, &extension_count, nullptr) != VK_SUCCESS) {
        return 0;
    }
    if (extension_count == 0) {
        return 0;
    }

    std::vector<VkExtensionProperties> extensions(extension_count);
    if (vkEnumerateInstanceExtensionProperties(nullptr, &extension_count, extensions.data()) != VK_SUCCESS) {
        return 0;
    }

    for (const auto& ext : extensions) {
        if (std::strcmp(ext.extensionName, extension_name) == 0) {
            return 1;
        }
    }

    return 0;
}

uint32_t qmv_get_instance_api_version() {
    uint32_t version = 0;
    if (vkEnumerateInstanceVersion(&version) != VK_SUCCESS) {
        return VK_API_VERSION_1_0;
    }
    return version;
}

}
