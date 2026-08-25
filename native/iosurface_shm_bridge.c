#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#define QMV_SHM_MAGIC 0x514D5631u
#define QMV_SHM_NAME_MAX 96
#define QMV_MAX_SURFACES 256

typedef struct {
    uint32_t magic;
    uint32_t width;
    uint32_t height;
    uint32_t stride;
    uint32_t pixel_format;
    uint32_t lock_state;
    uint64_t frame_counter;
    uint64_t data_size;
} qmv_surface_header_t;

typedef struct {
    int fd;
    char name[QMV_SHM_NAME_MAX];
    void *base;
    size_t mapped_size;
    qmv_surface_header_t *header;
    void *pixel_data;
    int owner;
    int external_fd;
} qmv_surface_t;

typedef struct {
    pthread_mutex_t lock;
    qmv_surface_t *slots[QMV_MAX_SURFACES];
} qmv_surface_table_t;

static qmv_surface_table_t g_table = { PTHREAD_MUTEX_INITIALIZER, { 0 } };

static size_t qmv_align_up(size_t value, size_t alignment) {
    return (value + alignment - 1) & ~(alignment - 1);
}

static int qmv_alloc_slot(qmv_surface_t *surface) {
    pthread_mutex_lock(&g_table.lock);
    int found = -1;
    for (int i = 0; i < QMV_MAX_SURFACES; i++) {
        if (g_table.slots[i] == NULL) {
            g_table.slots[i] = surface;
            found = i;
            break;
        }
    }
    pthread_mutex_unlock(&g_table.lock);
    return found;
}

static void qmv_free_slot_by_ptr(qmv_surface_t *surface) {
    pthread_mutex_lock(&g_table.lock);
    for (int i = 0; i < QMV_MAX_SURFACES; i++) {
        if (g_table.slots[i] == surface) {
            g_table.slots[i] = NULL;
            break;
        }
    }
    pthread_mutex_unlock(&g_table.lock);
}

qmv_surface_t *qmv_surface_create(const char *name, uint32_t width, uint32_t height,
                                    uint32_t stride, uint32_t pixel_format) {
    if (!name || width == 0 || height == 0 || stride == 0) {
        return NULL;
    }

    qmv_surface_t *surface = (qmv_surface_t *)calloc(1, sizeof(qmv_surface_t));
    if (!surface) {
        return NULL;
    }

    size_t payload_size = (size_t)stride * (size_t)height;
    size_t total_size = qmv_align_up(sizeof(qmv_surface_header_t) + payload_size, 4096);

    strncpy(surface->name, name, QMV_SHM_NAME_MAX - 1);
    surface->name[QMV_SHM_NAME_MAX - 1] = '\0';

    int fd = shm_open(surface->name, O_CREAT | O_RDWR | O_EXCL, 0600);
    if (fd < 0) {
        if (errno == EEXIST) {
            shm_unlink(surface->name);
            fd = shm_open(surface->name, O_CREAT | O_RDWR | O_EXCL, 0600);
        }
        if (fd < 0) {
            free(surface);
            return NULL;
        }
    }

    if (ftruncate(fd, (off_t)total_size) != 0) {
        close(fd);
        shm_unlink(surface->name);
        free(surface);
        return NULL;
    }

    void *base = mmap(NULL, total_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (base == MAP_FAILED) {
        close(fd);
        shm_unlink(surface->name);
        free(surface);
        return NULL;
    }

    qmv_surface_header_t *header = (qmv_surface_header_t *)base;
    header->magic = QMV_SHM_MAGIC;
    header->width = width;
    header->height = height;
    header->stride = stride;
    header->pixel_format = pixel_format;
    header->lock_state = 0;
    header->frame_counter = 0;
    header->data_size = payload_size;

    surface->fd = fd;
    surface->base = base;
    surface->mapped_size = total_size;
    surface->header = header;
    surface->pixel_data = (uint8_t *)base + sizeof(qmv_surface_header_t);
    surface->owner = 1;
    surface->external_fd = fd;

    int slot_id = qmv_alloc_slot(surface);
    if (slot_id < 0) {
        munmap(base, total_size);
        close(fd);
        shm_unlink(surface->name);
        free(surface);
        return NULL;
    }

    return surface;
}

qmv_surface_t *qmv_surface_open(const char *name) {
    if (!name) {
        return NULL;
    }

    qmv_surface_t *surface = (qmv_surface_t *)calloc(1, sizeof(qmv_surface_t));
    if (!surface) {
        return NULL;
    }

    strncpy(surface->name, name, QMV_SHM_NAME_MAX - 1);
    surface->name[QMV_SHM_NAME_MAX - 1] = '\0';

    int fd = shm_open(surface->name, O_RDWR, 0600);
    if (fd < 0) {
        free(surface);
        return NULL;
    }

    qmv_surface_header_t probe;
    if (read(fd, &probe, sizeof(probe)) != (ssize_t)sizeof(probe) || probe.magic != QMV_SHM_MAGIC) {
        close(fd);
        free(surface);
        return NULL;
    }
    lseek(fd, 0, SEEK_SET);

    size_t total_size = qmv_align_up(sizeof(qmv_surface_header_t) + probe.data_size, 4096);
    void *base = mmap(NULL, total_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (base == MAP_FAILED) {
        close(fd);
        free(surface);
        return NULL;
    }

    surface->fd = fd;
    surface->base = base;
    surface->mapped_size = total_size;
    surface->header = (qmv_surface_header_t *)base;
    surface->pixel_data = (uint8_t *)base + sizeof(qmv_surface_header_t);
    surface->owner = 0;
    surface->external_fd = fd;

    qmv_alloc_slot(surface);

    return surface;
}

void *qmv_surface_lock(qmv_surface_t *surface, uint64_t *out_size) {
    if (!surface || !surface->header) {
        return NULL;
    }
    __sync_fetch_and_or(&surface->header->lock_state, 1u);
    if (out_size) {
        *out_size = surface->header->data_size;
    }
    return surface->pixel_data;
}

void qmv_surface_unlock(qmv_surface_t *surface) {
    if (!surface || !surface->header) {
        return;
    }
    __sync_fetch_and_and(&surface->header->lock_state, ~1u);
    __sync_fetch_and_add(&surface->header->frame_counter, 1);
}

uint64_t qmv_surface_frame_counter(qmv_surface_t *surface) {
    if (!surface || !surface->header) {
        return 0;
    }
    return surface->header->frame_counter;
}

int qmv_surface_get_fd(qmv_surface_t *surface) {
    if (!surface) {
        return -1;
    }
    return surface->external_fd;
}

uint32_t qmv_surface_width(qmv_surface_t *surface) {
    return surface && surface->header ? surface->header->width : 0;
}

uint32_t qmv_surface_height(qmv_surface_t *surface) {
    return surface && surface->header ? surface->header->height : 0;
}

uint32_t qmv_surface_stride(qmv_surface_t *surface) {
    return surface && surface->header ? surface->header->stride : 0;
}

void qmv_surface_destroy(qmv_surface_t *surface) {
    if (!surface) {
        return;
    }

    if (surface->base && surface->base != MAP_FAILED) {
        munmap(surface->base, surface->mapped_size);
    }
    if (surface->fd >= 0) {
        close(surface->fd);
    }
    if (surface->owner) {
        shm_unlink(surface->name);
    }

    qmv_free_slot_by_ptr(surface);

    free(surface);
}

int qmv_surface_table_count(void) {
    int count = 0;
    pthread_mutex_lock(&g_table.lock);
    for (int i = 0; i < QMV_MAX_SURFACES; i++) {
        if (g_table.slots[i] != NULL) {
            count++;
        }
    }
    pthread_mutex_unlock(&g_table.lock);
    return count;
}
