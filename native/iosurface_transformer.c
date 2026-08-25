#include <mach/mach.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdint.h>
#include <stddef.h>

typedef struct {
    uint64_t client_token;
    mach_port_t surface_port;
    uint32_t width;
    uint32_t height;
    uint32_t frame_index;
    uint64_t received_at_mach_time;
} qmv_incoming_frame_t;

extern void *IOSurfaceLookupFromMachPort(mach_port_t port);
extern size_t IOSurfaceGetWidth(void *surface);
extern size_t IOSurfaceGetHeight(void *surface);
extern uint32_t IOSurfaceGetPixelFormat(void *surface);
extern size_t IOSurfaceGetBytesPerRow(void *surface);
extern void IOSurfaceRetain(void *surface);
extern void IOSurfaceRelease(void *surface);
extern Boolean IOSurfaceIsInUse(void *surface);

typedef struct {
    void *surface_ref;
    uint32_t width;
    uint32_t height;
    uint32_t bytes_per_row;
    uint32_t pixel_format;
} qmv_resolved_surface_t;

int qmv_resolve_surface_from_frame(const qmv_incoming_frame_t *frame, qmv_resolved_surface_t *out) {
    if (!frame || !out) {
        return -1;
    }
    if (frame->surface_port == MACH_PORT_NULL) {
        return -1;
    }

    void *surface_ref = IOSurfaceLookupFromMachPort(frame->surface_port);
    if (!surface_ref) {
        return -1;
    }

    size_t width = IOSurfaceGetWidth(surface_ref);
    size_t height = IOSurfaceGetHeight(surface_ref);

    if (width == 0 || height == 0) {
        IOSurfaceRelease(surface_ref);
        return -1;
    }

    if (frame->width != 0 && frame->height != 0) {
        if (width != frame->width || height != frame->height) {
            IOSurfaceRelease(surface_ref);
            return -1;
        }
    }

    out->surface_ref = surface_ref;
    out->width = (uint32_t)width;
    out->height = (uint32_t)height;
    out->bytes_per_row = (uint32_t)IOSurfaceGetBytesPerRow(surface_ref);
    out->pixel_format = IOSurfaceGetPixelFormat(surface_ref);

    return 0;
}

void *qmv_extract_surface_ref_from_frame(const qmv_incoming_frame_t *frame) {
    qmv_resolved_surface_t resolved;
    if (qmv_resolve_surface_from_frame(frame, &resolved) != 0) {
        return NULL;
    }
    return resolved.surface_ref;
}

void qmv_resolved_surface_release(qmv_resolved_surface_t *resolved) {
    if (!resolved || !resolved->surface_ref) {
        return;
    }
    IOSurfaceRelease(resolved->surface_ref);
    resolved->surface_ref = NULL;
}

int qmv_resolved_surface_is_in_use(const qmv_resolved_surface_t *resolved) {
    if (!resolved || !resolved->surface_ref) {
        return 0;
    }
    return IOSurfaceIsInUse(resolved->surface_ref) ? 1 : 0;
}
