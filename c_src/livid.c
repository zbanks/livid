#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "livid.h"

//static struct column * columns = NULL;
//static size_t columns_count = 0;

const struct column _columns[] = {
    {
        .name = "a",
        .cell_type = TEXT,
        .grid_show = true,
        .grid_width = 8,
    },
    {
        .name = "b",
        .cell_type = TEXT,
        .grid_show = true,
        .grid_width = 8,
    },
    {
        .name = "c",
        .cell_type = TEXT,
        .grid_show = true,
        .grid_width = 8,
    },
};
const size_t columns_count = sizeof(_columns) / sizeof(*_columns);
const struct column * columns = _columns;

//void
//setup(size_t _columns_count, struct column * _columns) {
//    columns_count = _columns_count;
//    columns = _columns;
//}

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

void
run(struct api * api) {
    //printf("hello world! %zu %p\n", _columns_count, columns_count);
    //printf("test %zu %zu %p\n", columns_count, sizeof(struct column), columns);
    struct cell * cells = NULL;
    while ((cells = api->next(api)) != NULL) {
        for (size_t i = 0; i < columns_count; i++) {
            //printf(" > %zu %zu %s %d\n",
            //       i, columns[i].index, columns[i].name, columns[i].cell_type);
            printf("[%zu %s %d] = '%s', ",
                    i, columns[i].name, columns[i].cell_type, cells[i].value.cell_text);
        }
        printf("\n");
        api->grid(api, cells);
    }
}


