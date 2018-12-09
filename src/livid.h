#ifndef __LIVID_H__
#define __LIVID_H__

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MIN(x, y) EXTCMP(x, y, <)
#define MAX(x, y) EXTCMP(x, y, >)
#define EXTCMP(x, y, op) ({ \
        __auto_type _x = (x); \
        __auto_type _y = (y); \
        (_x op _y) ? _x : _y; \
    })

#define PASTE(x, y) PASTE2(x, y)
#define PASTE2(x, y) x ## y

#define STRINGIFY(x) STRINGIFY2(x)
#define STRINGIFY2(x) #x

#define LOG(msg, ...) ({ \
        fprintf(log_file, __FILE__ ":%d:%s " msg "\n", __LINE__, __func__, ## __VA_ARGS__); \
        fflush(log_file); \
    })
#define ERRX(msg, ...) ({ \
        LOG(msg, ## __VA_ARGS__); \
        exit(EXIT_FAILURE); \
    })
#define ERR(msg, ...) ERRX(msg " (%s)", ## __VA_ARGS__, strerror(errno))

#define TYPE_LIST \
    TYPE(STR) \
    TYPE(TIME) \
    TYPE(LONG) \
    TYPE(DOUBLE) \

#define _TYPE_CTYPE_STR     const char *
#define _TYPE_LOWER_STR     str
#define _TYPE_CTYPE_TIME    long
#define _TYPE_LOWER_TIME    time
#define _TYPE_CTYPE_LONG    long
#define _TYPE_LOWER_LONG    long
#define _TYPE_CTYPE_DOUBLE  double
#define _TYPE_LOWER_DOUBLE  double
#define _TYPE_CTYPE(t) PASTE(_TYPE_CTYPE_, t)
#define _TYPE_LOWER(t) PASTE(_TYPE_LOWER_, t)

enum column_type {
    #define TYPE(t) PASTE(TYPE_, t),
    TYPE_LIST
    #undef TYPE
};

union cell {
    uint64_t _placeholder;
    #define TYPE(t) _TYPE_CTYPE(t) PASTE(cell_, _TYPE_LOWER(t));
    TYPE_LIST
    #undef TYPE
};
//static_assert(sizeof(union cell) == sizeof(uint64_t));

static inline const char * strtype(enum column_type type) {
    switch (type) {
        #define TYPE(t) case PASTE(TYPE_, t): return STRINGIFY(t);
        TYPE_LIST
        #undef TYPE
    }
    return "(unknown)";
}

extern FILE * log_file;
struct column {
    const char * const name;
    ssize_t index;
    enum column_type type;
    struct grid {
        bool hidden;
        size_t max_width;
    } grid;
};

#define SHOW(w) { .hidden = false, .max_width = (w) }
#define HIDE(w) { .hidden = true,  .max_width = (w) }
#define CELL_DEREF(cell, type) (cell.PASTE(cell_, _TYPE_LOWER(type)))
// COLUMN(name, type, hidden)

#ifdef COLUMN_LIST
struct row {
    #define COLUMN(_NAME, _TYPE, _HIDDEN) union { _TYPE_CTYPE(_TYPE) _NAME; uint64_t PASTE(_placeholder_, _NAME); };
    COLUMN_LIST
    #undef COLUMN
};
struct row_isvalid {
    #define COLUMN(_NAME, _TYPE, _HIDDEN) bool _NAME;
    COLUMN_LIST
    #undef COLUMN
};

#define COLUMN(_NAME, _TYPE, _HIDDEN) (struct column) { .name = STRINGIFY(_NAME), .type = PASTE(TYPE_, _TYPE), .grid = _HIDDEN},
struct column columns[] = {
    COLUMN_LIST
};
#undef COLUMN
const size_t columns_cnt = sizeof(columns) / sizeof(columns[0]);

#define next() (lv_next())
#define load_column(row, column) (lv_load(row, column.index))
#define load_all(row) (lv_load(row, -1))
#define write(msg, ...) lv_printf(msg "\n", ## __VA_ARGS__)
#define printf(...) lv_printf(__VA_ARGS__)
#define grid(row) lv_grid(columns, columns_cnt, row)

#define ROW_TYPE struct row
#else
#define ROW_TYPE union cell
#endif

bool lv_next(void);
int lv_load(ROW_TYPE * row, ssize_t column_index);
int lv_printf(const char * msg, ...);
int lv_grid(struct column * columns, size_t columns_cnt, ROW_TYPE * row);

#endif
