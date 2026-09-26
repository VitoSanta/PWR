# ringbuf

A fixed-capacity byte ring buffer in C99, no dependencies. The interface is
`include/ringbuf.h`; the implementation goes in `src/ringbuf.c`. It must
compile cleanly with `-std=c99 -Wall -Wextra -Werror -pedantic`.

- `ringbuf_new(capacity)`: a new empty buffer holding up to `capacity`
  bytes, or `NULL` when `capacity` is 0 or memory runs out.
  `ringbuf_free(NULL)` does nothing.
- `ringbuf_size`: bytes stored; `ringbuf_space`: `capacity - size`.
- `ringbuf_write(rb, data, len)`: appends as many of the `len` bytes as fit
  and returns how many that was (FIFO order).
- `ringbuf_write_overwrite(rb, data, len)`: appends all of them, dropping the
  oldest bytes to make room; when `len > capacity` only the last `capacity`
  bytes are kept. Returns how many old bytes were dropped.
- `ringbuf_read(rb, out, len)`: removes up to `len` of the oldest bytes into
  `out`, returns how many. `ringbuf_peek` is the same without removing.
  `out` may be `NULL` for `ringbuf_read` (then it discards).
- `ringbuf_discard(rb, len)`: removes up to `len` oldest bytes, returns how
  many.
- `ringbuf_find(rb, byte)`: the offset from the oldest byte of the first
  occurrence of `byte`, or -1.
- `ringbuf_read_line(rb, out, out_size)`: if the buffer holds a `'\n'`,
  removes everything up to and including it and copies the line *without*
  the newline into `out` as a NUL-terminated string, returning its length;
  a line longer than `out_size - 1` is removed but truncated to fit. If
  there is no newline it removes nothing and returns 0 with `out` set to
  `""`. `out_size` of 0 removes the line and writes nothing.

Every operation keeps working across the end of the underlying array
(wrap-around) for any sequence of calls.

Build and test with `make test`.
