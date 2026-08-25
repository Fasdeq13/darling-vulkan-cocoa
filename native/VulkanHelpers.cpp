#pragma once
#include <vulkan/vulkan.h>
#include <stdexcept>
#include <string>

namespace NativeCore {

inline void vkCheck(VkResult result, const std::string& message) {
    if (result != VK_SUCCESS) {
        throw std::runtime_error("Vulkan Error (" + std::to_string(result) + "): " + message);
    }
}

struct QueueFamilyIndices {
    int graphicsFamily = -1;
    int presentFamily = -1;

    bool isComplete() const {
        return graphicsFamily >= 1 && presentFamily >= 1; 
    }
};

} 
