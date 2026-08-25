#include <objc/runtime.h>
#include <objc/message.h>
#include <CoreFoundation/CoreFoundation.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    SEL selector;
    void *function_ptr;
} qmv_selector_cache_entry_t;

#define QMV_SELECTOR_CACHE_SIZE 128

static qmv_selector_cache_entry_t g_selector_cache[QMV_SELECTOR_CACHE_SIZE];
static int g_selector_cache_count = 0;
static pthread_mutex_t g_cache_lock = PTHREAD_MUTEX_INITIALIZER;

id qmv_objc_alloc_init(const char *class_name) {
    Class cls = objc_getClass(class_name);
    if (!cls) {
        return nil;
    }
    id instance = ((id (*)(id, SEL))objc_msgSend)((id)cls, sel_registerName("alloc"));
    instance = ((id (*)(id, SEL))objc_msgSend)(instance, sel_registerName("init"));
    return instance;
}

id qmv_objc_send_id(id target, const char *selector_name) {
    if (!target) {
        return nil;
    }
    SEL sel = sel_registerName(selector_name);
    return ((id (*)(id, SEL))objc_msgSend)(target, sel);
}

id qmv_objc_send_id_arg(id target, const char *selector_name, id argument) {
    if (!target) {
        return nil;
    }
    SEL sel = sel_registerName(selector_name);
    return ((id (*)(id, SEL, id))objc_msgSend)(target, sel, argument);
}

void qmv_objc_send_void(id target, const char *selector_name) {
    if (!target) {
        return;
    }
    SEL sel = sel_registerName(selector_name);
    ((void (*)(id, SEL))objc_msgSend)(target, sel);
}

void qmv_objc_send_void_arg(id target, const char *selector_name, id argument) {
    if (!target) {
        return;
    }
    SEL sel = sel_registerName(selector_name);
    ((void (*)(id, SEL, id))objc_msgSend)(target, sel, argument);
}

int64_t qmv_objc_send_int(id target, const char *selector_name) {
    if (!target) {
        return 0;
    }
    SEL sel = sel_registerName(selector_name);
    return ((int64_t (*)(id, SEL))objc_msgSend)(target, sel);
}

double qmv_objc_send_double(id target, const char *selector_name) {
    if (!target) {
        return 0.0;
    }
    SEL sel = sel_registerName(selector_name);
    return ((double (*)(id, SEL))objc_msgSend)(target, sel);
}

void qmv_objc_send_void_double(id target, const char *selector_name, double value) {
    if (!target) {
        return;
    }
    SEL sel = sel_registerName(selector_name);
    ((void (*)(id, SEL, double))objc_msgSend)(target, sel, value);
}

id qmv_objc_send_id_two_args(id target, const char *selector_name, id arg1, id arg2) {
    if (!target) {
        return nil;
    }
    SEL sel = sel_registerName(selector_name);
    return ((id (*)(id, SEL, id, id))objc_msgSend)(target, sel, arg1, arg2);
}

const char *qmv_nsstring_to_utf8(id ns_string) {
    if (!ns_string) {
        return NULL;
    }
    SEL sel = sel_registerName("UTF8String");
    return ((const char *(*)(id, SEL))objc_msgSend)(ns_string, sel);
}

id qmv_cstring_to_nsstring(const char *str) {
    if (!str) {
        return nil;
    }
    Class cls = objc_getClass("NSString");
    if (!cls) {
        return nil;
    }
    SEL sel = sel_registerName("stringWithUTF8String:");
    return ((id (*)(id, SEL, const char *))objc_msgSend)((id)cls, sel, str);
}

Class qmv_objc_get_class(const char *name) {
    return objc_getClass(name);
}

BOOL qmv_objc_class_respond_to(id target, const char *selector_name) {
    if (!target) {
        return NO;
    }
    SEL sel = sel_registerName(selector_name);
    return class_respondsToSelector(object_getClass(target), sel);
}

void qmv_objc_retain(id target) {
    if (!target) {
        return;
    }
    SEL sel = sel_registerName("retain");
    ((id (*)(id, SEL))objc_msgSend)(target, sel);
}

void qmv_objc_release(id target) {
    if (!target) {
        return;
    }
    SEL sel = sel_registerName("release");
    ((void (*)(id, SEL))objc_msgSend)(target, sel);
}

void *qmv_selector_cache_lookup(const char *selector_name) {
    SEL sel = sel_registerName(selector_name);
    void *result = NULL;

    pthread_mutex_lock(&g_cache_lock);
    for (int i = 0; i < g_selector_cache_count; i++) {
        if (g_selector_cache[i].selector == sel) {
            result = g_selector_cache[i].function_ptr;
            break;
        }
    }
    pthread_mutex_unlock(&g_cache_lock);

    return result;
}

int qmv_selector_cache_store(const char *selector_name, void *function_ptr) {
    SEL sel = sel_registerName(selector_name);
    int stored = 0;

    pthread_mutex_lock(&g_cache_lock);
    if (g_selector_cache_count < QMV_SELECTOR_CACHE_SIZE) {
        g_selector_cache[g_selector_cache_count].selector = sel;
        g_selector_cache[g_selector_cache_count].function_ptr = function_ptr;
        g_selector_cache_count++;
        stored = 1;
    }
    pthread_mutex_unlock(&g_cache_lock);

    return stored;
}

CFRunLoopRef qmv_current_run_loop(void) {
    return CFRunLoopGetCurrent();
}

void qmv_run_loop_run(void) {
    CFRunLoopRun();
}

void qmv_run_loop_stop(CFRunLoopRef loop) {
    if (loop) {
        CFRunLoopStop(loop);
    }
}
