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
#define _TYPE_LOWER_TEXT    text
#define _TYPE_CTYPE_TIME    long
#define _TYPE_LOWER_TIME    time
#define _TYPE_CTYPE_LONG    long
#define _TYPE_LOWER_LONG    long
#define _TYPE_CTYPE_DOUBLE  double
#define _TYPE_LOWER_DOUBLE  double
#define _TYPE_CTYPE(t) PASTE(_TYPE_CTYPE_, t)
#define _TYPE_LOWER(t) PASTE(_TYPE_LOWER_, t)
// XXX FIXME: Have rust do the capitalization
#define Text TEXT
#define Long LONG
#define Time TIME
#define Double DOUBLE

enum cell_type {
    #define TYPE(t, n) PASTE(TYPE_, t) = n,
    TYPE_LIST
    #undef TYPE
};

union cell_value {
    uint64_t _placeholder;
    #define TYPE(t, n) _TYPE_CTYPE(t) PASTE(cell_, _TYPE_LOWER(t));
    TYPE_LIST
    #undef TYPE
};
//static_assert(sizeof(union cell) == sizeof(uint64_t));

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
    bool grid_show;
    size_t grid_width;
};

struct cell {
    const struct column * const column;
    bool empty;
    union cell_value value;
};

#define SHOW true
#define HIDE false
#define CELL_DEREF(cell, type) (cell.PASTE(cell_, _TYPE_LOWER(type)))
// COLUMN(name, type, hidden)

struct row {
    #define COLUMN(_NAME, _TYPE, _SHOW) union { _TYPE_CTYPE(_TYPE) _NAME; uint64_t PASTE(_placeholder_, _NAME); };
    COLUMN_LIST
    #undef COLUMN
};
struct row_isvalid {
    #define COLUMN(_NAME, _TYPE, _SHOW) bool _NAME;
    COLUMN_LIST
    #undef COLUMN
};

#define COLUMN(_NAME, _TYPE, _SHOW) (struct column) { .name = STRINGIFY(_NAME), .cell_type = PASTE(TYPE_, _TYPE), .grid_show = _SHOW, .grid_width = 10},
const struct column columns[] = {
    COLUMN_LIST
};
#undef COLUMN
const size_t columns_count = sizeof(columns) / sizeof(columns[0]);

struct api;
struct api {
    struct cell * (* const next)(struct api * api);
    void (* const grid)(struct api * api, const struct cell * cells);
    void (* const write)(struct api * api, const char * str);
};

// Exports
//void setup(size_t columns_count, struct column * columns);
void run(struct api * api);

// ---
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

#define printf(...) api_printf(api, ## __VA_ARGS__)

#endif
