#include <mach/mach.h>
#include <mach/message.h>
#include <bootstrap.h>
#include <CoreFoundation/CoreFoundation.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

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
    mach_msg_header_t header;
    uint64_t client_token;
} qmv_goodbye_msg_t;

extern mach_port_t IOSurfaceCreateMachPort(void *surface);
extern int IOSurfaceGetID(void *surface);

typedef struct {
    mach_port_t server_port;
    mach_port_t local_reply_port;
    uint64_t token;
    int connected;
    pthread_mutex_t lock;
} qmv_client_connection_t;

static qmv_client_connection_t g_connection;
static int g_connection_initialized = 0;
static pthread_mutex_t g_init_lock = PTHREAD_MUTEX_INITIALIZER;

static qmv_client_connection_t *qmv_client_get(void) {
    pthread_mutex_lock(&g_init_lock);
    if (!g_connection_initialized) {
        memset(&g_connection, 0, sizeof(g_connection));
        g_connection.server_port = MACH_PORT_NULL;
        g_connection.local_reply_port = MACH_PORT_NULL;
        pthread_mutex_init(&g_connection.lock, NULL);
        g_connection_initialized = 1;
    }
    pthread_mutex_unlock(&g_init_lock);
    return &g_connection;
}

int qmv_client_connect(const char *service_name, uint64_t token) {
    if (!service_name) {
        return -1;
    }

    qmv_client_connection_t *conn = qmv_client_get();

    pthread_mutex_lock(&conn->lock);

    if (conn->connected) {
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    mach_port_t server_port = MACH_PORT_NULL;
    kern_return_t kr = bootstrap_look_up(bootstrap_port, service_name, &server_port);
    if (kr != KERN_SUCCESS) {
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    mach_port_t reply_port = MACH_PORT_NULL;
    kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &reply_port);
    if (kr != KERN_SUCCESS) {
        mach_port_deallocate(mach_task_self(), server_port);
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    kr = mach_port_insert_right(mach_task_self(), reply_port, reply_port, MACH_MSG_TYPE_MAKE_SEND);
    if (kr != KERN_SUCCESS) {
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), server_port);
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    qmv_hello_msg_t msg;
    memset(&msg, 0, sizeof(msg));
    msg.header.msgh_bits = MACH_MSGH_BITS_COMPLEX |
                            MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = sizeof(msg);
    msg.header.msgh_remote_port = server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = QMV_HELLO_MSG_ID;
    msg.body.msgh_descriptor_count = 1;
    msg.reply_port.name = reply_port;
    msg.reply_port.disposition = MACH_MSG_TYPE_MAKE_SEND;
    msg.reply_port.type = MACH_MSG_PORT_DESCRIPTOR;
    msg.client_token = token;
    msg.pid = (int32_t)getpid();

    kr = mach_msg(&msg.header, MACH_SEND_MSG, sizeof(msg), 0,
                  MACH_PORT_NULL, MACH_MSG_TIMEOUT_NONE, MACH_PORT_NULL);

    if (kr != KERN_SUCCESS) {
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), server_port);
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    conn->server_port = server_port;
    conn->local_reply_port = reply_port;
    conn->token = token;
    conn->connected = 1;

    pthread_mutex_unlock(&conn->lock);
    return 0;
}

int qmv_client_send_surface(uint64_t token, void *surface, uint32_t index) {
    if (!surface) {
        return -1;
    }

    qmv_client_connection_t *conn = qmv_client_get();

    pthread_mutex_lock(&conn->lock);
    if (!conn->connected || conn->token != token) {
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }
    mach_port_t server_port = conn->server_port;
    pthread_mutex_unlock(&conn->lock);

    mach_port_t surface_port = IOSurfaceCreateMachPort(surface);
    if (surface_port == MACH_PORT_NULL) {
        return -1;
    }

    uint32_t width = 0;
    uint32_t height = 0;

    qmv_frame_submit_msg_t msg;
    memset(&msg, 0, sizeof(msg));
    msg.header.msgh_bits = MACH_MSGH_BITS_COMPLEX |
                            MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = sizeof(msg);
    msg.header.msgh_remote_port = server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = QMV_FRAME_SUBMIT_MSG_ID;
    msg.body.msgh_descriptor_count = 1;
    msg.surface_port.name = surface_port;
    msg.surface_port.disposition = MACH_MSG_TYPE_MOVE_SEND;
    msg.surface_port.type = MACH_MSG_PORT_DESCRIPTOR;
    msg.client_token = token;
    msg.width = width;
    msg.height = height;
    msg.frame_index = index;

    kern_return_t kr = mach_msg(&msg.header, MACH_SEND_MSG, sizeof(msg), 0,
                                 MACH_PORT_NULL, MACH_MSG_TIMEOUT_NONE, MACH_PORT_NULL);

    if (kr != KERN_SUCCESS) {
        mach_port_deallocate(mach_task_self(), surface_port);
        return -1;
    }

    return 0;
}

int qmv_client_disconnect(uint64_t token) {
    qmv_client_connection_t *conn = qmv_client_get();

    pthread_mutex_lock(&conn->lock);
    if (!conn->connected || conn->token != token) {
        pthread_mutex_unlock(&conn->lock);
        return -1;
    }

    qmv_goodbye_msg_t msg;
    memset(&msg, 0, sizeof(msg));
    msg.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = sizeof(msg);
    msg.header.msgh_remote_port = conn->server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = QMV_GOODBYE_MSG_ID;
    msg.client_token = token;

    mach_msg(&msg.header, MACH_SEND_MSG, sizeof(msg), 0,
             MACH_PORT_NULL, MACH_MSG_TIMEOUT_NONE, MACH_PORT_NULL);

    mach_port_deallocate(mach_task_self(), conn->server_port);
    mach_port_mod_refs(mach_task_self(), conn->local_reply_port, MACH_PORT_RIGHT_RECEIVE, -1);

    conn->server_port = MACH_PORT_NULL;
    conn->local_reply_port = MACH_PORT_NULL;
    conn->connected = 0;

    pthread_mutex_unlock(&conn->lock);
    return 0;
}

int qmv_client_is_connected(uint64_t token) {
    qmv_client_connection_t *conn = qmv_client_get();
    pthread_mutex_lock(&conn->lock);
    int result = conn->connected && conn->token == token;
    pthread_mutex_unlock(&conn->lock);
    return result;
}