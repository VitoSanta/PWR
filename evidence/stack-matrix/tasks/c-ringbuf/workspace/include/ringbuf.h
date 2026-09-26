#ifndef RINGBUF_H
#define RINGBUF_H

#include <stddef.h>

/* A fixed-capacity byte ring buffer. See README.md. */
typedef struct ringbuf ringbuf;

ringbuf *ringbuf_new(size_t capacity);
void ringbuf_free(ringbuf *rb);

size_t ringbuf_capacity(const ringbuf *rb);
size_t ringbuf_size(const ringbuf *rb);
size_t ringbuf_space(const ringbuf *rb);

size_t ringbuf_write(ringbuf *rb, const void *data, size_t len);
size_t ringbuf_write_overwrite(ringbuf *rb, const void *data, size_t len);
size_t ringbuf_read(ringbuf *rb, void *out, size_t len);
size_t ringbuf_peek(const ringbuf *rb, void *out, size_t len);
size_t ringbuf_discard(ringbuf *rb, size_t len);
long ringbuf_find(const ringbuf *rb, unsigned char byte);
size_t ringbuf_read_line(ringbuf *rb, char *out, size_t out_size);

#endif
