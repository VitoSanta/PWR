#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "ringbuf.h"

/* A model of the buffer as a plain array, driven with the same random calls. */
int main(void) {
    unsigned char model[64];
    size_t model_len = 0;
    unsigned char data[32], out[64], expect[64];
    ringbuf *rb = ringbuf_new(37);
    srand(12345);
    for (int step = 0; step < 20000; step++) {
        size_t len = (size_t)(rand() % 20);
        for (size_t i = 0; i < len; i++) data[i] = (unsigned char)(rand() % 256);
        switch (rand() % 5) {
        case 0: {
            size_t fit = len < 37 - model_len ? len : 37 - model_len;
            assert(ringbuf_write(rb, data, len) == fit);
            memcpy(model + model_len, data, fit);
            model_len += fit;
            break;
        }
        case 1: {
            size_t total = model_len + len, drop = total > 37 ? total - 37 : 0;
            assert(ringbuf_write_overwrite(rb, data, len) == (drop > model_len ? model_len : drop));
            memcpy(model + model_len, data, len);
            memmove(model, model + drop, total - drop);
            model_len = total - drop;
            break;
        }
        case 2: {
            size_t n = len < model_len ? len : model_len;
            assert(ringbuf_read(rb, out, len) == n);
            assert(memcmp(out, model, n) == 0);
            memmove(model, model + n, model_len - n);
            model_len -= n;
            break;
        }
        case 3: {
            size_t n = len < model_len ? len : model_len;
            assert(ringbuf_peek(rb, out, len) == n && memcmp(out, model, n) == 0);
            unsigned char byte = data[0];
            long found = -1;
            for (size_t i = 0; i < model_len; i++) if (model[i] == byte) { found = (long)i; break; }
            assert(len == 0 || ringbuf_find(rb, byte) == found);
            break;
        }
        default: {
            size_t n = len < model_len ? len : model_len;
            assert(ringbuf_discard(rb, len) == n);
            memmove(model, model + n, model_len - n);
            model_len -= n;
        }
        }
        assert(ringbuf_size(rb) == model_len && ringbuf_space(rb) == 37 - model_len);
    }
    (void)expect;
    ringbuf_free(rb);

    char line[8];
    rb = ringbuf_new(4);
    ringbuf_write(rb, "ab\n", 3);
    assert(ringbuf_read_line(rb, line, 0) == 2 && ringbuf_size(rb) == 0);
    ringbuf_write(rb, "xy", 2);
    ringbuf_read(rb, NULL, 1);
    ringbuf_write(rb, "z\nq", 3); /* wraps: y z \n q */
    assert(ringbuf_read_line(rb, line, sizeof line) == 2 && strcmp(line, "yz") == 0);
    ringbuf_free(rb);
    puts("hidden ok");
    return 0;
}
