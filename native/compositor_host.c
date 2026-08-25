#include <mach/mach_time.h>
#include <mach/mach.h>
#include <mach/message.h>
#include <mach/mach_port.h>
#include <bootstrap.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define QMV_HOST_MAX_QUEUE 256
#define QMV_HOST_MAX_CLIENTS 64
#define QMV_HOST_MSG_BUF_SIZE 2048
#define QMV_FRAME_SUBMIT_MSG_ID 0x51004001u
#define QMV_HELLO_MSG_ID 0x51004002u
#define QMV_GOODBYE_MSG_ID 0x51004003u

typedef struct {
    mach_msg_header_t header;
    mach_msg_body_t body;
    mach_msg_port_descriptor_t surface_port;
    uint64_t client_token;
    uint32_t width;
    uint32_t height;
    uint32_t frame_index;
} qmv_frame_submit_msg_t;

typedef struct {
    mach_msg_header_t header;
    mach_msg_body_t body;
    mach_msg_port_descriptor_t reply_port;
    uint64_t client_token;
    int32_t pid;
} qmv_hello_msg_t;

typedef struct {
    uint64_t client_token;
    mach_port_t surface_port;
    uint32_t width;
    uint32_t height;
    uint32_t frame_index;
    uint64_t received_at_mach_time;
} qmv_incoming_frame_t;

typedef struct {
    uint64_t client_token;
    int32_t pid;
    mach_port_t reply_port;
    int active;
} qmv_client_record_t;

typedef void (*qmv_host_frame_callback_t)(const qmv_incoming_frame_t *frame, void *user_data);
typedef void (*qmv_host_client_callback_t)(uint64_t client_token, int32_t pid, int connected, void *user_data);

typedef struct {
    mach_port_t service_port;
    mach_port_t port_set;
    char service_name[128];
    pthread_t thread;
    pthread_mutex_t lock;
    volatile int running;

    qmv_incoming_frame_t queue[QMV_HOST_MAX_QUEUE];
    int queue_head;
    int queue_tail;
    int queue_count;

    qmv_client_record_t clients[QMV_HOST_MAX_CLIENTS];

    qmv_host_frame_callback_t frame_cb;
    void *frame_cb_user_data;
    qmv_host_client_callback_t client_cb;
    void *client_cb_user_data;
} qmv_compositor_host_t;

static qmv_client_record_t *qmv_host_find_client_locked(qmv_compositor_host_t *host, uint64_t token) {
    for (int i = 0; i < QMV_HOST_MAX_CLIENTS; i++) {
        if (host->clients[i].active && host->clients[i].client_token == token) {
            return &host->clients[i];
        }
    }
    return NULL;
}

static qmv_client_record_t *qmv_host_alloc_client_locked(qmv_compositor_host_t *host) {
    for (int i = 0; i < QMV_HOST_MAX_CLIENTS; i++) {
        if (!host->clients[i].active) {
            return &host->clients[i];
        }
    }
    return NULL;
}

qmv_compositor_host_t *qmv_compositor_host_create(const char *service_name) {
    if (!service_name) {
        return NULL;
    }

    qmv_compositor_host_t *host = (qmv_compositor_host_t *)calloc(1, sizeof(qmv_compositor_host_t));
    if (!host) {
        return NULL;
    }

    strncpy(host->service_name, service_name, sizeof(host->service_name) - 1);

    kern_return_t kr = bootstrap_check_in(bootstrap_port, host->service_name, &host->service_port);
    if (kr != KERN_SUCCESS) {
        kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &host->service_port);
        if (kr != KERN_SUCCESS) {
            free(host);
            return NULL;
        }
        mach_port_insert_right(mach_task_self(), host->service_port, host->service_port, MACH_MSG_TYPE_MAKE_SEND);
        bootstrap_register(bootstrap_port, host->service_name, host->service_port);
    }

    pthread_mutex_init(&host->lock, NULL);
    host->queue_head = 0;
    host->queue_tail = 0;
    host->queue_count = 0;
    host->running = 0;

    return host;
}

void qmv_compositor_host_set_frame_callback(qmv_compositor_host_t *host, qmv_host_frame_callback_t cb, void *user_data) {
    if (!host) {
        return;
    }
    host->frame_cb = cb;
    host->frame_cb_user_data = user_data;
}

void qmv_compositor_host_set_client_callback(qmv_compositor_host_t *host, qmv_host_client_callback_t cb, void *user_data) {
    if (!host) {
        return;
    }
    host->client_cb = cb;
    host->client_cb_user_data = user_data;
}

static void qmv_host_push_frame_locked(qmv_compositor_host_t *host, const qmv_incoming_frame_t *frame) {
    if (host->queue_count >= QMV_HOST_MAX_QUEUE) {
        qmv_incoming_frame_t dropped = host->queue[host->queue_head];
        host->queue_head = (host->queue_head + 1) % QMV_HOST_MAX_QUEUE;
        host->queue_count--;
        if (dropped.surface_port != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), dropped.surface_port);
        }
    }
    host->queue[host->queue_tail] = *frame;
    host->queue_tail = (host->queue_tail + 1) % QMV_HOST_MAX_QUEUE;
    host->queue_count++;
}

static void qmv_host_handle_hello(qmv_compositor_host_t *host, qmv_hello_msg_t *msg) {
    pthread_mutex_lock(&host->lock);
    qmv_client_record_t *existing = qmv_host_find_client_locked(host, msg->client_token);
    if (existing) {
        if (existing->reply_port != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), existing->reply_port);
        }
        existing->reply_port = msg->reply_port.name;
        existing->pid = msg->pid;
        pthread_mutex_unlock(&host->lock);
        return;
    }

    qmv_client_record_t *slot = qmv_host_alloc_client_locked(host);
    if (!slot) {
        pthread_mutex_unlock(&host->lock);
        if (msg->reply_port.name != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), msg->reply_port.name);
        }
        return;
    }

    slot->active = 1;
    slot->client_token = msg->client_token;
    slot->pid = msg->pid;
    slot->reply_port = msg->reply_port.name;
    pthread_mutex_unlock(&host->lock);

    if (host->client_cb) {
        host->client_cb(msg->client_token, msg->pid, 1, host->client_cb_user_data);
    }
}

static void qmv_host_handle_goodbye(qmv_compositor_host_t *host, uint64_t client_token) {
    pthread_mutex_lock(&host->lock);
    qmv_client_record_t *client = qmv_host_find_client_locked(host, client_token);
    int32_t pid = client ? client->pid : -1;
    if (client) {
        if (client->reply_port != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), client->reply_port);
        }
        client->active = 0;
        client->reply_port = MACH_PORT_NULL;
    }
    pthread_mutex_unlock(&host->lock);

    if (client && host->client_cb) {
        host->client_cb(client_token, pid, 0, host->client_cb_user_data);
    }
}

static void qmv_host_handle_frame_submit(qmv_compositor_host_t *host, qmv_frame_submit_msg_t *msg) {
    pthread_mutex_lock(&host->lock);
    qmv_client_record_t *client = qmv_host_find_client_locked(host, msg->client_token);
    pthread_mutex_unlock(&host->lock);

    if (!client) {
        if (msg->surface_port.name != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), msg->surface_port.name);
        }
        return;
    }

    qmv_incoming_frame_t frame;
    frame.client_token = msg->client_token;
    frame.surface_port = msg->surface_port.name;
    frame.width = msg->width;
    frame.height = msg->height;
    frame.frame_index = msg->frame_index;
    frame.received_at_mach_time = mach_absolute_time();

    pthread_mutex_lock(&host->lock);
    qmv_host_push_frame_locked(host, &frame);
    pthread_mutex_unlock(&host->lock);

    if (host->frame_cb) {
        host->frame_cb(&frame, host->frame_cb_user_data);
    }
}

static void qmv_host_dispatch(qmv_compositor_host_t *host, mach_msg_header_t *header) {
    switch (header->msgh_id) {
        case QMV_HELLO_MSG_ID:
            qmv_host_handle_hello(host, (qmv_hello_msg_t *)header);
            break;
        case QMV_GOODBYE_MSG_ID: {
            uint64_t *body = (uint64_t *)((uint8_t *)header + sizeof(mach_msg_header_t));
            qmv_host_handle_goodbye(host, *body);
            break;
        }
        case QMV_FRAME_SUBMIT_MSG_ID:
            qmv_host_handle_frame_submit(host, (qmv_frame_submit_msg_t *)header);
            break;
        default:
            break;
    }
}

int qmv_compositor_host_run_once(qmv_compositor_host_t *host, int timeout_ms) {
    if (!host) {
        return -1;
    }

    uint8_t buf[QMV_HOST_MSG_BUF_SIZE];
    mach_msg_header_t *header = (mach_msg_header_t *)buf;

    mach_msg_option_t opts = MACH_RCV_MSG | MACH_RCV_TIMEOUT;
    mach_msg_return_t mr = mach_msg(header, opts, 0, QMV_HOST_MSG_BUF_SIZE,
                                      host->service_port, (mach_msg_timeout_t)timeout_ms, MACH_PORT_NULL);

    if (mr == MACH_RCV_TIMED_OUT) {
        return 0;
    }
    if (mr != MACH_MSG_SUCCESS) {
        return -1;
    }

    qmv_host_dispatch(host, header);
    return 1;
}

static void *qmv_compositor_host_thread_main(void *arg) {
    qmv_compositor_host_t *host = (qmv_compositor_host_t *)arg;
    while (host->running) {
        qmv_compositor_host_run_once(host, 50);
    }
    return NULL;
}

int qmv_compositor_host_start(qmv_compositor_host_t *host) {
    if (!host || host->running) {
        return -1;
    }
    host->running = 1;
    return pthread_create(&host->thread, NULL, qmv_compositor_host_thread_main, host);
}

void qmv_compositor_host_stop(qmv_compositor_host_t *host) {
    if (!host || !host->running) {
        return;
    }
    host->running = 0;
    pthread_join(host->thread, NULL);
}

int qmv_compositor_host_pop_frame(qmv_compositor_host_t *host, qmv_incoming_frame_t *out_frame) {
    if (!host || !out_frame) {
        return 0;
    }

    pthread_mutex_lock(&host->lock);
    if (host->queue_count == 0) {
        pthread_mutex_unlock(&host->lock);
        return 0;
    }

    *out_frame = host->queue[host->queue_head];
    host->queue_head = (host->queue_head + 1) % QMV_HOST_MAX_QUEUE;
    host->queue_count--;
    pthread_mutex_unlock(&host->lock);

    return 1;
}

void qmv_compositor_host_release_frame_port(qmv_incoming_frame_t *frame) {
    if (!frame || frame->surface_port == MACH_PORT_NULL) {
        return;
    }
    mach_port_deallocate(mach_task_self(), frame->surface_port);
    frame->surface_port = MACH_PORT_NULL;
}

int qmv_compositor_host_client_count(qmv_compositor_host_t *host) {
    if (!host) {
        return 0;
    }
    int count = 0;
    pthread_mutex_lock(&host->lock);
    for (int i = 0; i < QMV_HOST_MAX_CLIENTS; i++) {
        if (host->clients[i].active) {
            count++;
        }
    }
    pthread_mutex_unlock(&host->lock);
    return count;
}

mach_port_t qmv_compositor_host_get_service_port(qmv_compositor_host_t *host) {
    if (!host) {
        return MACH_PORT_NULL;
    }
    return host->service_port;
}

void qmv_compositor_host_destroy(qmv_compositor_host_t *host) {
    if (!host) {
        return;
    }

    qmv_compositor_host_stop(host);

    pthread_mutex_lock(&host->lock);
    while (host->queue_count > 0) {
        qmv_incoming_frame_t frame = host->queue[host->queue_head];
        host->queue_head = (host->queue_head + 1) % QMV_HOST_MAX_QUEUE;
        host->queue_count--;
        if (frame.surface_port != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), frame.surface_port);
        }
    }
    for (int i = 0; i < QMV_HOST_MAX_CLIENTS; i++) {
        if (host->clients[i].active && host->clients[i].reply_port != MACH_PORT_NULL) {
            mach_port_deallocate(mach_task_self(), host->clients[i].reply_port);
        }
    }
    pthread_mutex_unlock(&host->lock);

    pthread_mutex_destroy(&host->lock);

    if (host->service_port != MACH_PORT_NULL) {
        mach_port_mod_refs(mach_task_self(), host->service_port, MACH_PORT_RIGHT_RECEIVE, -1);
    }

    free(host);
}
