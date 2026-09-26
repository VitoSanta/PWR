#include "ringbuf.h"

#include <stdlib.h>
#include <string.h>

struct ringbuf {
    unsigned char *data;
    size_t capacity;
    size_t head; /* index of the oldest byte */
    size_t size;
};

ringbuf *ringbuf_new(size_t capacity) {
    ringbuf *rb;
    if (capacity == 0) return NULL;
    rb = malloc(sizeof *rb);
    if (!rb) return NULL;
    rb->data = malloc(capacity);
    if (!rb->data) {
        free(rb);
        return NULL;
    }
    rb->capacity = capacity;
    rb->head = 0;
    rb->size = 0;
    return rb;
}

void ringbuf_free(ringbuf *rb) {
    if (!rb) return;
    free(rb->data);
    free(rb);
}

size_t ringbuf_capacity(const ringbuf *rb) { return rb->capacity; }
size_t ringbuf_size(const ringbuf *rb) { return rb->size; }
size_t ringbuf_space(const ringbuf *rb) { return rb->capacity - rb->size; }

static void put(ringbuf *rb, const unsigned char *data, size_t len) {
    size_t tail = (rb->head + rb->size) % rb->capacity;
    size_t first = rb->capacity - tail < len ? rb->capacity - tail : len;
    memcpy(rb->data + tail, data, first);
    memcpy(rb->data, data + first, len - first);
    rb->size += len;
}

size_t ringbuf_write(ringbuf *rb, const void *data, size_t len) {
    size_t n = len < ringbuf_space(rb) ? len : ringbuf_space(rb);
    put(rb, data, n);
    return n;
}

size_t ringbuf_peek(const ringbuf *rb, void *out, size_t len) {
    size_t n = len < rb->size ? len : rb->size;
    size_t first = rb->capacity - rb->head < n ? rb->capacity - rb->head : n;
    if (out) {
        memcpy(out, rb->data + rb->head, first);
        memcpy((unsigned char *)out + first, rb->data, n - first);
    }
    return n;
}

size_t ringbuf_discard(ringbuf *rb, size_t len) {
    size_t n = len < rb->size ? len : rb->size;
    rb->head = (rb->head + n) % rb->capacity;
    rb->size -= n;
    return n;
}

size_t ringbuf_read(ringbuf *rb, void *out, size_t len) {
    size_t n = ringbuf_peek(rb, out, len);
    return ringbuf_discard(rb, n);
}

size_t ringbuf_write_overwrite(ringbuf *rb, const void *data, size_t len) {
    const unsigned char *bytes = data;
    size_t dropped = 0;
    if (len > rb->capacity) {
        dropped = rb->size;
        rb->head = 0;
        rb->size = 0;
        bytes += len - rb->capacity;
        len = rb->capacity;
    } else if (len > ringbuf_space(rb)) {
        dropped = ringbuf_discard(rb, len - ringbuf_space(rb));
    }
    put(rb, bytes, len);
    return dropped;
}

long ringbuf_find(const ringbuf *rb, unsigned char byte) {
    size_t i;
    for (i = 0; i < rb->size; i++) {
        if (rb->data[(rb->head + i) % rb->capacity] == byte) return (long)i;
    }
    return -1;
}

size_t ringbuf_read_line(ringbuf *rb, char *out, size_t out_size) {
    long newline = ringbuf_find(rb, '\n');
    size_t length, copy;
    if (newline < 0) {
        if (out && out_size > 0) out[0] = '\0';
        return 0;
    }
    length = (size_t)newline;
    if (out && out_size > 0) {
        copy = length < out_size - 1 ? length : out_size - 1;
        ringbuf_peek(rb, out, copy);
        out[copy] = '\0';
    }
    ringbuf_discard(rb, length + 1);
    return out_size > 0 && length > out_size - 1 ? out_size - 1 : length;
}
