#ifndef __LIVID_H__
#define __LIVID_H__

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#define PASTE(x, y) PASTE2(x, y)
#define PASTE2(x, y) x ## y

#define STRINGIFY(x) STRINGIFY2(x)
#define STRINGIFY2(x) #x

// Types

#define TYPE_LIST \
    TYPE(TEXT, 0) \
    TYPE(LONG, 1) \
    TYPE(TIME, 2) \
    TYPE(DOUBLE, 3) \

#define _TYPE_CTYPE_TEXT    const char *
#define _TYPE_CTYPE_TIME    long
#define _TYPE_CTYPE_LONG    long
#define _TYPE_CTYPE_DOUBLE  double
#define _TYPE_CTYPE(t) PASTE(_TYPE_CTYPE_, t)

enum cell_type {
    #define TYPE(t, n) PASTE(TYPE_, t) = n,
    TYPE_LIST
    #undef TYPE
};

#define TYPE(t, _n) _Static_assert(sizeof(_TYPE_CTYPE(t)) <= sizeof(uint64_t), "Each value type must be less than 8 bytes: " t " with type " STRINGIFY(_TYPE_CTYPE(t)) " is too big");
TYPE_LIST
#undef TYPE

static inline const char * strtype(enum cell_type type) {
    switch (type) {
        #define TYPE(t, n) case PASTE(TYPE_, t): return STRINGIFY(t);
        TYPE_LIST
        #undef TYPE
    }
    return "(unknown)";
}

struct column {
    const char * const name;
    enum cell_type cell_type;
    int16_t grid_width;
};

#define GRID_WIDTH(n)   (n)
#define GRID_HIDDEN     -1
#define GRID_AUTO       0

struct row {
    #define COLUMN(_NAME, _TYPE, _GRID_WIDTH) union { _TYPE_CTYPE(_TYPE) _NAME; uint64_t PASTE(_placeholder_, _NAME); };
    COLUMN_LIST
    #undef COLUMN
    struct {
        #define COLUMN(_NAME, _TYPE, _GRID_WIDTH) bool _NAME;
        COLUMN_LIST
        #undef COLUMN
    } _empty;
};

struct api;
struct api {
    int8_t (* const next)(struct api * api, void * row_out, bool * empty_out);
    int8_t (* const grid)(struct api * api, const void * row, const bool * empty);
    void (* const write)(struct api * api, const char * str);

    char _rust_owned_data[];
};

// Exports
void run(struct api * api);

#define COLUMN(_NAME, _TYPE, _GRID_WIDTH) (struct column) { .name = STRINGIFY(_NAME), .cell_type = PASTE(TYPE_, _TYPE), .grid_width = _GRID_WIDTH},
const struct column columns[] = {
    COLUMN_LIST
};
#undef COLUMN
const size_t columns_count = sizeof(columns) / sizeof(columns[0]);

// Functions to use inside script
static bool
api_next(struct api * const api, struct row * const row) {
    return api->next(api, row, (bool *) &row->_empty);
}

static bool
api_grid(struct api * const api, struct row const * const row) {
    return api->grid(api, row, (bool *) &row->_empty);
}

#define printf(...) api_printf(api, ## __VA_ARGS__)
static void
api_printf(struct api * const api, const char * const fmt, ...) {
    va_list vargs;
    va_start(vargs, fmt);
    int len = vsnprintf(NULL, 0, fmt, vargs) + 1;
    if (len < 0) return;

    static char * buf = NULL;
    static size_t buflen = 0;
    if (buflen < (size_t) len) {
        size_t new_buflen = (size_t) len * 2;
        char * new_buf = realloc(buf, new_buflen);
        if(!new_buf) {
            fprintf(stderr, "Unable to realloc %zu bytes for api_printf", new_buflen);
            return;
        }
        buf = new_buf;
        buflen = new_buflen;
    }

    va_end(vargs);
    va_start(vargs, fmt);
    vsnprintf(buf, buflen, fmt, vargs);
    va_end(vargs);

    buf[buflen-1] = '\0';
    api->write(api, buf);
}

#endif
