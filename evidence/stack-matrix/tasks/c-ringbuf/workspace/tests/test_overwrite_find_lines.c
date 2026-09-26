#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "ringbuf.h"

int main(void) {
    char out[64];
    ringbuf *rb = ringbuf_new(5);
    assert(ringbuf_write_overwrite(rb, "abc", 3) == 0);
    assert(ringbuf_write_overwrite(rb, "defg", 4) == 2);
    assert(ringbuf_peek(rb, out, 5) == 5 && memcmp(out, "cdefg", 5) == 0);
    assert(ringbuf_write_overwrite(rb, "0123456789", 10) == 5);
    assert(ringbuf_peek(rb, out, 5) == 5 && memcmp(out, "56789", 5) == 0);

    assert(ringbuf_find(rb, '7') == 2);
    assert(ringbuf_find(rb, 'x') == -1);
    assert(ringbuf_discard(rb, 3) == 3);
    assert(ringbuf_find(rb, '9') == 1);
    assert(ringbuf_read(rb, NULL, 10) == 2 && ringbuf_size(rb) == 0);
    ringbuf_free(rb);

    rb = ringbuf_new(16);
    ringbuf_write(rb, "one\ntwo", 7);
    assert(ringbuf_read_line(rb, out, sizeof out) == 3 && strcmp(out, "one") == 0);
    assert(ringbuf_read_line(rb, out, sizeof out) == 0 && strcmp(out, "") == 0);
    assert(ringbuf_size(rb) == 3);
    ringbuf_write(rb, "\nabcdefgh\n", 10);
    assert(ringbuf_read_line(rb, out, sizeof out) == 3 && strcmp(out, "two") == 0);
    assert(ringbuf_read_line(rb, out, 4) == 3 && strcmp(out, "abc") == 0);
    assert(ringbuf_size(rb) == 0);
    ringbuf_free(rb);
    puts("overwrite, find and lines ok");
    return 0;
}
