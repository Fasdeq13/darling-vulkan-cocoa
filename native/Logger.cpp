#pragma once
#include <iostream>
#include <string>

namespace NativeCore {

enum class LogLevel {
    Info,
    Warning,
    Error
};

class Logger {
public:
    static void log(LogLevel level, const std::string& message) {
        switch (level) {
            case LogLevel::Info:
                std::cout << "[INFO] " << message << std::endl;
                break;
            case LogLevel::Warning:
                std::cerr << "[WARNING] " << message << std::endl;
                break;
            case LogLevel::Error:
                std::cerr << "[ERROR] " << message << std::endl;
                break;
        }
    }
};

}
