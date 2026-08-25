#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdatomic.h>
#include <time.h>

#define QMV_SHM_SYNC_MAGIC 0x514D5653u
#define QMV_SHM_SYNC_NAME_MAX 96
#define QMV_SHM_SYNC_MAX_SLOTS 8

typedef enum {
    QMV_FRAME_FREE = 0,
    QMV_FRAME_WRITING = 1,
    QMV_FRAME_READY = 2,
    QMV_FRAME_READING = 3
} qmv_frame_state_t;

typedef struct {
    _Atomic uint32_t state;
    _Atomic uint64_t generation;
    uint32_t frame_index;
    uint32_t width;
    uint32_t height;
} qmv_shm_slot_t;

typedef struct {
    uint32_t magic;
    uint32_t slot_count;
    _Atomic uint32_t write_cursor;
    _Atomic uint32_t read_cursor;
    qmv_shm_slot_t slots[QMV_SHM_SYNC_MAX_SLOTS];
} qmv_shm_sync_region_t;

typedef struct {
    int fd;
    char name[QMV_SHM_SYNC_NAME_MAX];
    void *base;
    size_t mapped_size;
    qmv_shm_sync_region_t *region;
    int owner;
} qmv_shm_sync_t;

static size_t qmv_shm_sync_align_up(size_t value, size_t alignment) {
    return (value + alignment - 1) & ~(alignment - 1);
}

qmv_shm_sync_t *qmv_shm_sync_create(const char *name, uint32_t slot_count) {
    if (!name || slot_count == 0 || slot_count > QMV_SHM_SYNC_MAX_SLOTS) {
        return NULL;
    }

    qmv_shm_sync_t *sync = (qmv_shm_sync_t *)calloc(1, sizeof(qmv_shm_sync_t));
    if (!sync) {
        return NULL;
    }

    strncpy(sync->name, name, QMV_SHM_SYNC_NAME_MAX - 1);
    sync->name[QMV_SHM_SYNC_NAME_MAX - 1] = '\0';

    int fd = shm_open(sync->name, O_CREAT | O_RDWR | O_EXCL, 0600);
    if (fd < 0) {
        if (errno == EEXIST) {
            shm_unlink(sync->name);
            fd = shm_open(sync->name, O_CREAT | O_RDWR | O_EXCL, 0600);
        }
        if (fd < 0) {
            free(sync);
            return NULL;
        }
    }

    size_t total_size = qmv_shm_sync_align_up(sizeof(qmv_shm_sync_region_t), 4096);
    if (ftruncate(fd, (off_t)total_size) != 0) {
        close(fd);
        shm_unlink(sync->name);
        free(sync);
        return NULL;
    }

    void *base = mmap(NULL, total_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (base == MAP_FAILED) {
        close(fd);
        shm_unlink(sync->name);
        free(sync);
        return NULL;
    }

    qmv_shm_sync_region_t *region = (qmv_shm_sync_region_t *)base;
    region->magic = QMV_SHM_SYNC_MAGIC;
    region->slot_count = slot_count;
    atomic_store(&region->write_cursor, 0);
    atomic_store(&region->read_cursor, 0);
    for (uint32_t i = 0; i < slot_count; i++) {
        atomic_store(&region->slots[i].state, QMV_FRAME_FREE);
        atomic_store(&region->slots[i].generation, 0);
        region->slots[i].frame_index = 0;
        region->slots[i].width = 0;
        region->slots[i].height = 0;
    }

    sync->fd = fd;
    sync->base = base;
    sync->mapped_size = total_size;
    sync->region = region;
    sync->owner = 1;

    return sync;
}

qmv_shm_sync_t *qmv_shm_sync_open(const char *name) {
    if (!name) {
        return NULL;
    }

    qmv_shm_sync_t *sync = (qmv_shm_sync_t *)calloc(1, sizeof(qmv_shm_sync_t));
    if (!sync) {
        return NULL;
    }

    strncpy(sync->name, name, QMV_SHM_SYNC_NAME_MAX - 1);
    sync->name[QMV_SHM_SYNC_NAME_MAX - 1] = '\0';

    int fd = shm_open(sync->name, O_RDWR, 0600);
    if (fd < 0) {
        free(sync);
        return NULL;
    }

    size_t total_size = qmv_shm_sync_align_up(sizeof(qmv_shm_sync_region_t), 4096);
    void *base = mmap(NULL, total_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (base == MAP_FAILED) {
        close(fd);
        free(sync);
        return NULL;
    }

    qmv_shm_sync_region_t *region = (qmv_shm_sync_region_t *)base;
    if (region->magic != QMV_SHM_SYNC_MAGIC) {
        munmap(base, total_size);
        close(fd);
        free(sync);
        return NULL;
    }

    sync->fd = fd;
    sync->base = base;
    sync->mapped_size = total_size;
    sync->region = region;
    sync->owner = 0;

    return sync;
}

static int qmv_shm_sync_try_transition(qmv_shm_slot_t *slot, qmv_frame_state_t expected, qmv_frame_state_t desired) {
    uint32_t exp = (uint32_t)expected;
    return atomic_compare_exchange_strong(&slot->state, &exp, (uint32_t)desired);
}

int qmv_shm_sync_acquire_write_slot(qmv_shm_sync_t *sync, uint32_t *out_slot_index) {
    if (!sync || !sync->region || !out_slot_index) {
        return -1;
    }

    qmv_shm_sync_region_t *region = sync->region;
    for (uint32_t attempt = 0; attempt < region->slot_count; attempt++) {
        uint32_t idx = atomic_fetch_add(&region->write_cursor, 1) % region->slot_count;
        qmv_shm_slot_t *slot = &region->slots[idx];
        if (qmv_shm_sync_try_transition(slot, QMV_FRAME_FREE, QMV_FRAME_WRITING)) {
            *out_slot_index = idx;
            return 0;
        }
    }
    return -1;
}

int qmv_shm_sync_publish_slot(qmv_shm_sync_t *sync, uint32_t slot_index, uint32_t frame_index,
                               uint32_t width, uint32_t height) {
    if (!sync || !sync->region || slot_index >= sync->region->slot_count) {
        return -1;
    }

    qmv_shm_slot_t *slot = &sync->region->slots[slot_index];
    if (!qmv_shm_sync_try_transition(slot, QMV_FRAME_WRITING, QMV_FRAME_READY)) {
        return -1;
    }

    slot->frame_index = frame_index;
    slot->width = width;
    slot->height = height;
    atomic_fetch_add(&slot->generation, 1);

    return 0;
}

int qmv_shm_sync_acquire_read_slot(qmv_shm_sync_t *sync, uint32_t *out_slot_index) {
    if (!sync || !sync->region || !out_slot_index) {
        return -1;
    }

    qmv_shm_sync_region_t *region = sync->region;
    for (uint32_t i = 0; i < region->slot_count; i++) {
        qmv_shm_slot_t *slot = &region->slots[i];
        if (qmv_shm_sync_try_transition(slot, QMV_FRAME_READY, QMV_FRAME_READING)) {
            *out_slot_index = i;
            return 0;
        }
    }
    return -1;
}

int qmv_shm_sync_release_read_slot(qmv_shm_sync_t *sync, uint32_t slot_index) {
    if (!sync || !sync->region || slot_index >= sync->region->slot_count) {
        return -1;
    }

    qmv_shm_slot_t *slot = &sync->region->slots[slot_index];
    if (!qmv_shm_sync_try_transition(slot, QMV_FRAME_READING, QMV_FRAME_FREE)) {
        return -1;
    }
    return 0;
}

int qmv_shm_sync_get_slot_info(qmv_shm_sync_t *sync, uint32_t slot_index, uint32_t *out_frame_index,
                                uint32_t *out_width, uint32_t *out_height) {
    if (!sync || !sync->region || slot_index >= sync->region->slot_count) {
        return -1;
    }

    qmv_shm_slot_t *slot = &sync->region->slots[slot_index];
    if (out_frame_index) {
        *out_frame_index = slot->frame_index;
    }
    if (out_width) {
        *out_width = slot->width;
    }
    if (out_height) {
        *out_height = slot->height;
    }
    return 0;
}

int qmv_shm_sync_wait_for_ready(qmv_shm_sync_t *sync, uint32_t timeout_ms, uint32_t *out_slot_index) {
    if (!sync || !out_slot_index) {
        return -1;
    }

    struct timespec start;
    clock_gettime(CLOCK_MONOTONIC, &start);

    for (;;) {
        if (qmv_shm_sync_acquire_read_slot(sync, out_slot_index) == 0) {
            return 0;
        }

        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        long elapsed_ms = (now.tv_sec - start.tv_sec) * 1000 +
                           (now.tv_nsec - start.tv_nsec) / 1000000;
        if (elapsed_ms >= (long)timeout_ms) {
            return -1;
        }

        struct timespec sleep_time;
        sleep_time.tv_sec = 0;
        sleep_time.tv_nsec = 500000;
        nanosleep(&sleep_time, NULL);
    }
}

int qmv_shm_sync_get_fd(qmv_shm_sync_t *sync) {
    if (!sync) {
        return -1;
    }
    return sync->fd;
}

void qmv_shm_sync_destroy(qmv_shm_sync_t *sync) {
    if (!sync) {
        return;
    }

    if (sync->base && sync->base != MAP_FAILED) {
        munmap(sync->base, sync->mapped_size);
    }
    if (sync->fd >= 0) {
        close(sync->fd);
    }
    if (sync->owner) {
        shm_unlink(sync->name);
    }

    free(sync);
}