#CC=gcc
#CC=clang
#CC=afl-clang-fast

CFLAGS = -std=c11 -Wall -Wextra -Wconversion -Werror -D_POSIX_C_SOURCE=201804L -I.
CFLAGS += -ggdb3 -O0
#CFLAGS += -O3

liblivid.so: livid.c | livid.h
	$(CC) $(CFLAGS) $^ -shared -fPIC -ldl -o $@

.PHONY: clean
clean:
	-rm -f *.o *.so

.PHONY: all
all: liblivid.so

.DEFAULT_GOAL = all
