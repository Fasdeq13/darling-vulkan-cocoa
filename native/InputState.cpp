#pragma once

namespace NativeCore {

struct MousePosition {
    double x = 0.0;
    double y = 0.0;
};

class InputState {
public:
    static InputState& getInstance() {
        static InputState instance;
        return instance;
    }

    void setKeyPressed(int key, bool pressed) {
        if (key >= 0 && key < 512) {
            m_keys[key] = pressed;
        }
    }

    bool isKeyPressed(int key) const {
        if (key >= 0 && key < 512) {
            return m_keys[key];
        }
        return false;
    }

    }

    MousePosition getMousePosition() const {
        return m_mousePos;
    }

private:
    InputState() = default;
    bool m_keys[512] = {false};
    MousePosition m_mousePos = {0.0, 0.0};
};


