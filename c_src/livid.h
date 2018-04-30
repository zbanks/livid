#ifndef __LIVID_H__
#define __LIVID_H__

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Types

enum cell_type {
    TEXT = 0,
    LONG = 1,
    TIME = 2,
    DOUBLE = 3,
};

union cell_value {
    const char * cell_text;
    int64_t cell_long;
    int64_t cell_time;
    double cell_double;
};

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

struct api;
struct api {
    struct cell * (* const next)(struct api * api);
    void (* const grid)(struct api * api, const struct cell * cells);
    void (* const write)(struct api * api, const char * str);
};

// Exports
//void setup(size_t columns_count, struct column * columns);
void run(struct api * api);
const struct column * columns;
const size_t columns_count;

#endif
