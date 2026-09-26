CC ?= cc
CFLAGS ?= -std=c99 -Wall -Wextra -Werror -pedantic -g -Iinclude
TESTS := $(patsubst tests/%.c,build/%,$(wildcard tests/*.c))

.PHONY: test clean
test: $(TESTS)
	@for t in $(TESTS); do echo "== $$t"; ./$$t || exit 1; done
	@echo "ALL TESTS PASSED"

build/%: tests/%.c src/ringbuf.c include/ringbuf.h
	@mkdir -p build
	$(CC) $(CFLAGS) -o $@ $< src/ringbuf.c

clean:
	rm -rf build
