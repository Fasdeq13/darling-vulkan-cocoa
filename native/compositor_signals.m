#import <Cocoa/Cocoa.h>
#import <mach/mach_time.h>
#import <Foundation/Foundation.h>
#import <mach/mach.h>
#import <mach/message.h>
#import <stdint.h>
#import <string.h>

#define QMV_INPUT_EVENT_MSG_ID 0x51004010u

typedef enum {
    QMV_INPUT_MOUSE_MOVED = 1,
    QMV_INPUT_MOUSE_DOWN = 2,
    QMV_INPUT_MOUSE_UP = 3,
    QMV_INPUT_MOUSE_DRAGGED = 4,
    QMV_INPUT_SCROLL_WHEEL = 5,
    QMV_INPUT_KEY_DOWN = 6,
    QMV_INPUT_KEY_UP = 7,
    QMV_INPUT_FLAGS_CHANGED = 8
} qmv_input_kind_t;

typedef struct {
    uint32_t kind;
    uint64_t client_token;
    uint64_t window_id;
    uint64_t timestamp_mach;
    double x;
    double y;
    double delta_x;
    double delta_y;
    uint32_t button_number;
    uint32_t key_code;
    uint32_t modifier_flags;
    uint32_t click_count;
    uint16_t characters_utf16[16];
    uint32_t characters_length;
} qmv_input_event_t;

typedef struct {
    mach_msg_header_t header;
    qmv_input_event_t event;
} qmv_input_event_msg_t;

static qmv_input_event_t qmv_make_base_event(qmv_input_kind_t kind, uint64_t client_token, uint64_t window_id) {
    qmv_input_event_t event;
    memset(&event, 0, sizeof(event));
    event.kind = (uint32_t)kind;
    event.client_token = client_token;
    event.window_id = window_id;
    event.timestamp_mach = mach_absolute_time();
    return event;
}

static int qmv_send_input_event(mach_port_t reply_port, const qmv_input_event_t *event) {
    if (reply_port == MACH_PORT_NULL || !event) {
        return -1;
    }

    qmv_input_event_msg_t msg;
    memset(&msg, 0, sizeof(msg));
    msg.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = sizeof(msg);
    msg.header.msgh_remote_port = reply_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = QMV_INPUT_EVENT_MSG_ID;
    msg.event = *event;

    mach_msg_return_t kr = mach_msg(&msg.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT, sizeof(msg), 0,
                                     MACH_PORT_NULL, 50, MACH_PORT_NULL);

    return kr == MACH_MSG_SUCCESS ? 0 : -1;
}

int qmv_signals_send_mouse_moved(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                  double x, double y, double delta_x, double delta_y) {
    qmv_input_event_t event = qmv_make_base_event(QMV_INPUT_MOUSE_MOVED, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_send_mouse_button(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                   int is_down, uint32_t button_number, double x, double y,
                                   uint32_t click_count, uint32_t modifier_flags) {
    qmv_input_event_t event = qmv_make_base_event(is_down ? QMV_INPUT_MOUSE_DOWN : QMV_INPUT_MOUSE_UP,
                                                   client_token, window_id);
    event.x = x;
    event.y = y;
    event.button_number = button_number;
    event.click_count = click_count;
    event.modifier_flags = modifier_flags;
    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_send_mouse_dragged(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                    double x, double y, double delta_x, double delta_y,
                                    uint32_t button_number) {
    qmv_input_event_t event = qmv_make_base_event(QMV_INPUT_MOUSE_DRAGGED, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    event.button_number = button_number;
    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_send_scroll_wheel(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                   double x, double y, double delta_x, double delta_y) {
    qmv_input_event_t event = qmv_make_base_event(QMV_INPUT_SCROLL_WHEEL, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_send_key_event(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                int is_down, uint32_t key_code, uint32_t modifier_flags,
                                const uint16_t *utf16_chars, uint32_t utf16_length) {
    qmv_input_event_t event = qmv_make_base_event(is_down ? QMV_INPUT_KEY_DOWN : QMV_INPUT_KEY_UP,
                                                   client_token, window_id);
    event.key_code = key_code;
    event.modifier_flags = modifier_flags;

    uint32_t copy_length = utf16_length;
    if (copy_length > 16) {
        copy_length = 16;
    }
    if (utf16_chars && copy_length > 0) {
        memcpy(event.characters_utf16, utf16_chars, copy_length * sizeof(uint16_t));
    }
    event.characters_length = copy_length;

    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_send_flags_changed(mach_port_t reply_port, uint64_t client_token, uint64_t window_id,
                                    uint32_t modifier_flags) {
    qmv_input_event_t event = qmv_make_base_event(QMV_INPUT_FLAGS_CHANGED, client_token, window_id);
    event.modifier_flags = modifier_flags;
    return qmv_send_input_event(reply_port, &event);
}

int qmv_signals_translate_nsevent(id ns_event, uint64_t client_token, uint64_t window_id,
                                   mach_port_t reply_port) {
    if (!ns_event || reply_port == MACH_PORT_NULL) {
        return -1;
    }

    NSEvent *evt = (NSEvent *)ns_event;
    NSEventType type = evt.type;
    NSPoint loc = evt.locationInWindow;
    uint32_t mods = (uint32_t)evt.modifierFlags;

    switch (type) {
        case NSEventTypeMouseMoved:
            return qmv_signals_send_mouse_moved(reply_port, client_token, window_id,
                                                 loc.x, loc.y, evt.deltaX, evt.deltaY);
        case NSEventTypeLeftMouseDown:
        case NSEventTypeRightMouseDown:
        case NSEventTypeOtherMouseDown:
            return qmv_signals_send_mouse_button(reply_port, client_token, window_id, 1,
                                                  (uint32_t)evt.buttonNumber, loc.x, loc.y,
                                                  (uint32_t)evt.clickCount, mods);
        case NSEventTypeLeftMouseUp:
        case NSEventTypeRightMouseUp:
        case NSEventTypeOtherMouseUp:
            return qmv_signals_send_mouse_button(reply_port, client_token, window_id, 0,
                                                  (uint32_t)evt.buttonNumber, loc.x, loc.y,
                                                  (uint32_t)evt.clickCount, mods);
        case NSEventTypeLeftMouseDragged:
        case NSEventTypeRightMouseDragged:
        case NSEventTypeOtherMouseDragged:
            return qmv_signals_send_mouse_dragged(reply_port, client_token, window_id,
                                                   loc.x, loc.y, evt.deltaX, evt.deltaY,
                                                   (uint32_t)evt.buttonNumber);
        case NSEventTypeScrollWheel:
            return qmv_signals_send_scroll_wheel(reply_port, client_token, window_id,
                                                  loc.x, loc.y, evt.scrollingDeltaX, evt.scrollingDeltaY);
        case NSEventTypeKeyDown:
        case NSEventTypeKeyUp: {
            NSString *chars = evt.characters;
            uint16_t buf[16];
            uint32_t len = 0;
            if (chars) {
                NSUInteger n = chars.length;
                if (n > 16) {
                    n = 16;
                }
                [chars getCharacters:buf range:NSMakeRange(0, n)];
                len = (uint32_t)n;
            }
            return qmv_signals_send_key_event(reply_port, client_token, window_id,
                                               type == NSEventTypeKeyDown, (uint32_t)evt.keyCode,
                                               mods, buf, len);
        }
        case NSEventTypeFlagsChanged:
            return qmv_signals_send_flags_changed(reply_port, client_token, window_id, mods);
        default:
            return -1;
    }
}
