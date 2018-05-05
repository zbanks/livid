
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


