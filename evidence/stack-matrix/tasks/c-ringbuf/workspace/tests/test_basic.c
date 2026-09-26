#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "ringbuf.h"

int main(void) {
    char out[64];
    ringbuf *rb = ringbuf_new(8);
    assert(rb && ringbuf_capacity(rb) == 8 && ringbuf_size(rb) == 0 && ringbuf_space(rb) == 8);
    assert(ringbuf_new(0) == NULL);
    ringbuf_free(NULL);

    assert(ringbuf_write(rb, "abcdef", 6) == 6);
    assert(ringbuf_write(rb, "ghijk", 5) == 2);
    assert(ringbuf_size(rb) == 8 && ringbuf_space(rb) == 0);

    assert(ringbuf_peek(rb, out, 3) == 3 && memcmp(out, "abc", 3) == 0);
    assert(ringbuf_read(rb, out, 5) == 5 && memcmp(out, "abcde", 5) == 0);
    assert(ringbuf_size(rb) == 3);

    /* Wraps around the end of the array. */
    assert(ringbuf_write(rb, "12345", 5) == 5);
    assert(ringbuf_read(rb, out, 64) == 8 && memcmp(out, "fgh12345", 8) == 0);
    assert(ringbuf_read(rb, out, 1) == 0);
    ringbuf_free(rb);
    puts("basic ok");
    return 0;
}
