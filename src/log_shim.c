/*
 * C shim bridging FFmpeg's va_list-based log callback to the Rust side.
 */

#include <libavutil/log.h>

#include <stdarg.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

/* Matches FFmpeg's own LINE_SZ: the size of a single formatted fragment. */
#define LINE_SIZE 1024

void codecpod_log_dispatch(int level, const char *text, size_t len);

static atomic_flag lock = ATOMIC_FLAG_INIT;
static char *acc;
static size_t acc_len;
static size_t acc_cap;
static int line_level;
static int print_prefix = 1;

static void acquire(void) {
    while (atomic_flag_test_and_set_explicit(&lock, memory_order_acquire)) {
    }
}

static void release(void) {
    atomic_flag_clear_explicit(&lock, memory_order_release);
}

static int reserve(size_t need) {
    if (need <= acc_cap) {
        return 0;
    }
    size_t cap = acc_cap ? acc_cap : LINE_SIZE;
    while (cap < need) {
        cap *= 2;
    }
    char *grown = realloc(acc, cap);
    if (!grown) {
        return -1;
    }
    acc = grown;
    acc_cap = cap;
    return 0;
}

static void codecpod_log_callback(void *avcl, int level, const char *fmt, va_list vl) {
    char frag[LINE_SIZE];

    acquire();

    int written = av_log_format_line2(avcl, level, fmt, vl, frag, LINE_SIZE, &print_prefix);
    if (written < 0) {
        release();
        return;
    }

    /* `written` is the length that would have been produced; clamp to what fit. */
    int truncated = written >= LINE_SIZE;
    size_t flen = truncated ? (size_t)(LINE_SIZE - 1) : (size_t)written;

    if (acc_len == 0) {
        line_level = level;
    }
    if (reserve(acc_len + flen) != 0) {
        release();
        return;
    }
    memcpy(acc + acc_len, frag, flen);
    acc_len += flen;

    /* A line is complete once FFmpeg emits its trailing newline; a truncated
     * fragment is treated as complete since its own newline was cut off. */
    int complete = truncated || (flen > 0 && frag[flen - 1] == '\n');
    if (!complete) {
        release();
        return;
    }

    /* Trim trailing CR/LF, then detach the finished line so the shared buffer is
     * free for the next line before we drop the lock. */
    while (acc_len > 0 && (acc[acc_len - 1] == '\n' || acc[acc_len - 1] == '\r')) {
        acc_len--;
    }
    size_t out_len = acc_len;
    int out_level = line_level;
    char *out = malloc(out_len + 1);
    if (out) {
        memcpy(out, acc, out_len);
        out[out_len] = '\0';
    }
    acc_len = 0;

    release();

    if (out) {
        codecpod_log_dispatch(out_level, out, out_len);
        free(out);
    }
}

void codecpod_log_reset(void) {
    acquire();
    acc_len = 0;
    print_prefix = 1;
    release();
}

void codecpod_log_install(void) {
    av_log_set_callback(codecpod_log_callback);
}

void codecpod_log_restore(int level) {
    av_log_set_level(level);
    av_log_set_callback(av_log_default_callback);
}

__attribute__((constructor)) static void codecpod_log_default_quiet(void) {
    av_log_set_level(AV_LOG_QUIET);
}
