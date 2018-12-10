// TEXT, LONG, TIME, DOUBLE
// GRID_AUTO, GRID_HIDDEN, GRID_WIDTH(12)
#include "livid.h"
const size_t grid_rows_limit = 20;

void run(struct api * api) {
    printf("hello\n");
    struct row row[1];
    while (api_next(api, row)) {
        //row->b *= 2;
        //row->_empty.c = row->a & 1;
        if (api_grid(api, row)) return;
    }
}
