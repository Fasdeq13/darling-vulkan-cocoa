#include <vulkan/vulkan.h>
#include <cstdint>
#include <cstdio>
#include <cstring>

extern "C" {

typedef void (*QmvLogCallback)(int32_t severity, const char* message);

static QmvLogCallback g_log_callback = nullptr;

static VKAPI_ATTR VkBool32 VKAPI_CALL qmv_debug_callback(
    VkDebugUtilsMessageSeverityFlagBitsEXT message_severity,
    VkDebugUtilsMessageTypeFlagsEXT message_type,
    const VkDebugUtilsMessengerCallbackDataEXT* callback_data,
    void* user_data) {

    (void)message_type;
    (void)user_data;

    int32_t severity_code = 0;
    if (message_severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT) {
        severity_code = 3;
    } else if (message_severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT) {
        severity_code = 2;
    } else if (message_severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_INFO_BIT_EXT) {
        severity_code = 1;
    }

    if (g_log_callback != nullptr && callback_data != nullptr && callback_data->pMessage != nullptr) {
        g_log_callback(severity_code, callback_data->pMessage);
    } else if (callback_data != nullptr && callback_data->pMessage != nullptr) {
        std::fprintf(stderr, "[vulkan][%d] %s\n", severity_code, callback_data->pMessage);
    }

    return VK_FALSE;
}

void qmv_set_debug_log_callback(QmvLogCallback callback) {
    g_log_callback = callback;
}

VkResult qmv_create_debug_messenger(VkInstance instance, VkDebugUtilsMessengerEXT* out_messenger) {
    if (instance == VK_NULL_HANDLE || out_messenger == nullptr) {
        return VK_ERROR_INITIALIZATION_FAILED;
    }

    auto create_fn = reinterpret_cast<PFN_vkCreateDebugUtilsMessengerEXT>(
        vkGetInstanceProcAddr(instance, "vkCreateDebugUtilsMessengerEXT"));

    if (create_fn == nullptr) {
        return VK_ERROR_EXTENSION_NOT_PRESENT;
    }

    VkDebugUtilsMessengerCreateInfoEXT create_info{};
    create_info.sType = VK_STRUCTURE_TYPE_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT;
    create_info.messageSeverity =
        VK_DEBUG_UTILS_MESSAGE_SEVERITY_VERBOSE_BIT_EXT |
        VK_DEBUG_UTILS_MESSAGE_SEVERITY_INFO_BIT_EXT |
        VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT |
        VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT;
    create_info.messageType =
        VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT |
        VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT |
        VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT;
    create_info.pfnUserCallback = qmv_debug_callback;
    create_info.pUserData = nullptr;

    return create_fn(instance, &create_info, nullptr, out_messenger);
}

void qmv_destroy_debug_messenger(VkInstance instance, VkDebugUtilsMessengerEXT messenger) {
    if (instance == VK_NULL_HANDLE || messenger == VK_NULL_HANDLE) {
        return;
    }

    auto destroy_fn = reinterpret_cast<PFN_vkDestroyDebugUtilsMessengerEXT>(
        vkGetInstanceProcAddr(instance, "vkDestroyDebugUtilsMessengerEXT"));

    if (destroy_fn != nullptr) {
        destroy_fn(instance, messenger, nullptr);
    }
}

int32_t qmv_check_validation_layer_support(const char* layer_name) {
    if (layer_name == nullptr) {
        return 0;
    }

    uint32_t layer_count = 0;
    vkEnumerateInstanceLayerProperties(&layer_count, nullptr);

    if (layer_count == 0) {
        return 0;
    }

    VkLayerProperties* layers = new VkLayerProperties[layer_count];
    vkEnumerateInstanceLayerProperties(&layer_count, layers);

    int32_t found = 0;
    for (uint32_t i = 0; i < layer_count; i++) {
        if (std::strcmp(layers[i].layerName, layer_name) == 0) {
            found = 1;
            break;
        }
    }

    delete[] layers;
    return found;
}

}
