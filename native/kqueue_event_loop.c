#ifndef NOTE_MSECONDS
#define NOTE_MSECONDS 0x00000001
#endif
#include <sys/types.h>
#include <sys/event.h>
#include <sys/time.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

#define QMV_MAX_EVENTS 64
#define QMV_MAX_TIMERS 32

typedef void (*qmv_fd_callback_t)(int fd, int16_t filter, intptr_t data, void *user_data);
typedef void (*qmv_timer_callback_t)(uint64_t timer_id, void *user_data);
typedef void (*qmv_mach_callback_t)(mach_port_t port, void *msg_buf, size_t msg_size, void *user_data);

typedef struct {
    uint64_t id;
    int active;
    qmv_timer_callback_t callback;
    void *user_data;
} qmv_timer_entry_t;

typedef struct {
    int kq;
    mach_port_t mach_recv_port;
    mach_port_t mach_port_set;
    pthread_t thread;
    pthread_mutex_t lock;
    volatile int running;
    qmv_fd_callback_t fd_cb;
    qmv_mach_callback_t mach_cb;
    void *fd_cb_user_data;
    void *mach_cb_user_data;
    qmv_timer_entry_t timers[QMV_MAX_TIMERS];
    uint64_t next_timer_id;
} qmv_event_loop_t;

static qmv_event_loop_t *g_loop = NULL;

static qmv_timer_entry_t *qmv_find_timer_slot(qmv_event_loop_t *loop, uint64_t id) {
    for (int i = 0; i < QMV_MAX_TIMERS; i++) {
        if (loop->timers[i].active && loop->timers[i].id == id) {
            return &loop->timers[i];
        }
    }
    return NULL;
}

static qmv_timer_entry_t *qmv_alloc_timer_slot(qmv_event_loop_t *loop) {
    for (int i = 0; i < QMV_MAX_TIMERS; i++) {
        if (!loop->timers[i].active) {
            return &loop->timers[i];
        }
    }
    return NULL;
}

qmv_event_loop_t *qmv_event_loop_create(void) {
    qmv_event_loop_t *loop = (qmv_event_loop_t *)calloc(1, sizeof(qmv_event_loop_t));
    if (!loop) {
        return NULL;
    }

    loop->kq = kqueue();
    if (loop->kq < 0) {
        free(loop);
        return NULL;
    }

    kern_return_t kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &loop->mach_recv_port);
    if (kr != KERN_SUCCESS) {
        close(loop->kq);
        free(loop);
        return NULL;
    }

    kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_PORT_SET, &loop->mach_port_set);
    if (kr == KERN_SUCCESS) {
        mach_port_insert_member(mach_task_self(), loop->mach_recv_port, loop->mach_port_set);
    }

    struct kevent64_s reg;
    EV_SET64(&reg, loop->mach_port_set, EVFILT_MACHPORT, EV_ADD | EV_ENABLE,
              MACH_RCV_MSG, 0, 0, 0, 0);
    if (kevent64(loop->kq, &reg, 1, NULL, 0, 0, NULL) != 0) {
        mach_port_deallocate(mach_task_self(), loop->mach_port_set);
        mach_port_deallocate(mach_task_self(), loop->mach_recv_port);
        close(loop->kq);
        free(loop);
        return NULL;
    }

    pthread_mutex_init(&loop->lock, NULL);
    loop->next_timer_id = 1;
    loop->running = 0;
    return loop;
}

void qmv_event_loop_destroy(qmv_event_loop_t *loop) {
    if (!loop) {
        return;
    }
    loop->running = 0;
    if (loop->mach_port_set != MACH_PORT_NULL) {
        mach_port_deallocate(mach_task_self(), loop->mach_port_set);
    }
    if (loop->mach_recv_port != MACH_PORT_NULL) {
        mach_port_deallocate(mach_task_self(), loop->mach_recv_port);
    }
    if (loop->kq >= 0) {
        close(loop->kq);
    }
    pthread_mutex_destroy(&loop->lock);
    free(loop);
}

int qmv_event_loop_watch_fd(qmv_event_loop_t *loop, int fd, int16_t filter) {
    if (!loop) {
        return -1;
    }
    struct kevent64_s reg;
    EV_SET64(&reg, (uint64_t)fd, filter, EV_ADD | EV_ENABLE | EV_CLEAR, 0, 0, 0, 0, 0);
    return kevent64(loop->kq, &reg, 1, NULL, 0, 0, NULL);
}

int qmv_event_loop_unwatch_fd(qmv_event_loop_t *loop, int fd, int16_t filter) {
    if (!loop) {
        return -1;
    }
    struct kevent64_s reg;
    EV_SET64(&reg, (uint64_t)fd, filter, EV_DELETE, 0, 0, 0, 0, 0);
    return kevent64(loop->kq, &reg, 1, NULL, 0, 0, NULL);
}

void qmv_event_loop_set_fd_callback(qmv_event_loop_t *loop, qmv_fd_callback_t cb, void *user_data) {
    if (!loop) {
        return;
    }
    loop->fd_cb = cb;
    loop->fd_cb_user_data = user_data;
}

void qmv_event_loop_set_mach_callback(qmv_event_loop_t *loop, qmv_mach_callback_t cb, void *user_data) {
    if (!loop) {
        return;
    }
    loop->mach_cb = cb;
    loop->mach_cb_user_data = user_data;
}

mach_port_t qmv_event_loop_get_mach_port(qmv_event_loop_t *loop) {
    if (!loop) {
        return MACH_PORT_NULL;
    }
    return loop->mach_recv_port;
}

uint64_t qmv_event_loop_add_timer(qmv_event_loop_t *loop, uint64_t interval_ms, qmv_timer_callback_t cb, void *user_data) {
    if (!loop) {
        return 0;
    }

    pthread_mutex_lock(&loop->lock);
    qmv_timer_entry_t *slot = qmv_alloc_timer_slot(loop);
    if (!slot) {
        pthread_mutex_unlock(&loop->lock);
        return 0;
    }

    uint64_t id = loop->next_timer_id++;
    slot->id = id;
    slot->active = 1;
    slot->callback = cb;
    slot->user_data = user_data;
    pthread_mutex_unlock(&loop->lock);

    struct kevent64_s reg;
    EV_SET64(&reg, id, EVFILT_TIMER, EV_ADD | EV_ENABLE, NOTE_MSECONDS, (int64_t)interval_ms, 0, 0, 0);
    if (kevent64(loop->kq, &reg, 1, NULL, 0, 0, NULL) != 0) {
        pthread_mutex_lock(&loop->lock);
        slot->active = 0;
        pthread_mutex_unlock(&loop->lock);
        return 0;
    }

    return id;
}

void qmv_event_loop_remove_timer(qmv_event_loop_t *loop, uint64_t timer_id) {
    if (!loop) {
        return;
    }

    struct kevent64_s reg;
    EV_SET64(&reg, timer_id, EVFILT_TIMER, EV_DELETE, 0, 0, 0, 0, 0);
    kevent64(loop->kq, &reg, 1, NULL, 0, 0, NULL);

    pthread_mutex_lock(&loop->lock);
    qmv_timer_entry_t *slot = qmv_find_timer_slot(loop, timer_id);
    if (slot) {
        slot->active = 0;
    }
    pthread_mutex_unlock(&loop->lock);
}

static void qmv_dispatch_mach_message(qmv_event_loop_t *loop) {
    mach_msg_header_t *buf = (mach_msg_header_t *)malloc(4096);
    if (!buf) {
        return;
    }

    mach_msg_return_t mr = mach_msg(buf, MACH_RCV_MSG | MACH_RCV_TIMEOUT, 0, 4096,
                                      loop->mach_recv_port, 0, MACH_PORT_NULL);
    if (mr == MACH_MSG_SUCCESS) {
        if (loop->mach_cb) {
            loop->mach_cb(loop->mach_recv_port, buf, buf->msgh_size, loop->mach_cb_user_data);
        }
    }

    free(buf);
}

static void qmv_dispatch_kevent(qmv_event_loop_t *loop, struct kevent64_s *ev) {
    if (ev->filter == EVFILT_MACHPORT) {
        qmv_dispatch_mach_message(loop);
        return;
    }

    if (ev->filter == EVFILT_TIMER) {
        uint64_t id = ev->ident;
        pthread_mutex_lock(&loop->lock);
        qmv_timer_entry_t *slot = qmv_find_timer_slot(loop, id);
        qmv_timer_callback_t cb = slot ? slot->callback : NULL;
        void *ud = slot ? slot->user_data : NULL;
        pthread_mutex_unlock(&loop->lock);
        if (cb) {
            cb(id, ud);
        }
        return;
    }

    if (loop->fd_cb) {
        loop->fd_cb((int)ev->ident, ev->filter, (intptr_t)ev->data, loop->fd_cb_user_data);
    }
}

int qmv_event_loop_run_once(qmv_event_loop_t *loop, int timeout_ms) {
    if (!loop) {
        return -1;
    }

    struct kevent64_s events[QMV_MAX_EVENTS];
    struct timespec ts;
    struct timespec *ts_ptr = NULL;

    if (timeout_ms >= 0) {
        ts.tv_sec = timeout_ms / 1000;
        ts.tv_nsec = (timeout_ms % 1000) * 1000000L;
        ts_ptr = &ts;
    }

    int n = kevent64(loop->kq, NULL, 0, events, QMV_MAX_EVENTS, 0, ts_ptr);
    if (n < 0) {
        if (errno == EINTR) {
            return 0;
        }
        return -1;
    }

    for (int i = 0; i < n; i++) {
        qmv_dispatch_kevent(loop, &events[i]);
    }

    return n;
}

static void *qmv_event_loop_thread_main(void *arg) {
    qmv_event_loop_t *loop = (qmv_event_loop_t *)arg;
    while (loop->running) {
        qmv_event_loop_run_once(loop, 50);
    }
    return NULL;
}

int qmv_event_loop_start(qmv_event_loop_t *loop) {
    if (!loop || loop->running) {
        return -1;
    }
    loop->running = 1;
    return pthread_create(&loop->thread, NULL, qmv_event_loop_thread_main, loop);
}

void qmv_event_loop_stop(qmv_event_loop_t *loop) {
    if (!loop || !loop->running) {
        return;
    }
    loop->running = 0;
    pthread_join(loop->thread, NULL);
}

qmv_event_loop_t *qmv_event_loop_global(void) {
    if (!g_loop) {
        g_loop = qmv_event_loop_create();
    }
    return g_loop;
}

void qmv_event_loop_global_destroy(void) {
    if (g_loop) {
        qmv_event_loop_destroy(g_loop);
        g_loop = NULL;
    }
}
