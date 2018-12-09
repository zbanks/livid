#CC=gcc
#CC=clang
#CC=afl-clang-fast
TARGET=livid

CFLAGS = -std=c11 -Wall -Wextra -Wconversion -Werror -D_POSIX_C_SOURCE=201804L -Isrc/
CFLAGS += -ggdb3 -O0
#CFLAGS += -O3

src/livid.h.inc: src/livid.h
	xxd -i $< $@

liblivid.so: src/livid.c src/livid_reader.c src/livid_editor.c src/optim.c | src/livid.h.inc src/livid.h
	$(CC) $(CFLAGS) $^ -shared -fPIC -ldl -o $@

livid: liblivid.so
	$(CC) $(CFLAGS) -Wl,-rpath='$$ORIGIN' -L. $^ -llivid -ldl -o $@

.PHONY: clean
clean:
	-rm -f *.o *.so $(TARGET) src/livid.h.inc

.PHONY: all
all: $(TARGET)

.DEFAULT_GOAL = all
