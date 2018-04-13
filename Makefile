#CC=gcc
#CC=clang
#CC=afl-clang-fast
TARGET=livid

CFLAGS = -std=c11 -Wall -Wextra -Wconversion -Werror -D_POSIX_C_SOURCE=201804L -Isrc/
CFLAGS += -ggdb3 -O0
#CFLAGS += -O3

liblivid.so: src/livid.c src/optim.c
	$(CC) $(CFLAGS) $^ -shared -fPIC -ldl -o $@

livid: liblivid.so
	$(CC) $(CFLAGS) -Wl,-rpath='$$ORIGIN' -L. $^ -llivid -ldl -o $@

.PHONY: clean
clean:
	-rm -f *.o $(TARGET)

.PHONY: all
all: $(TARGET)

.DEFAULT_GOAL = all
