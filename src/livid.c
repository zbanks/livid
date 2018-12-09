#include <dlfcn.h>
#include <errno.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/inotify.h>
#include <sys/poll.h>

#include "optim.h"
#include "livid.h"
#include "livid_editor.h"
#include "livid.h.inc"

// TODO:
// * Sanitize column names
// * Clean up row struct / is_valid bits (is_loaded?)
// * `row` is local but `row_isvalid` is global?
// * Incremental loading
// * Abstract reader (that supports pipes & storing)
// * Map script.c column names to local column names
// * Support efficient seeking (via next/load_column)
// * Generate re-runable (static?) binary from script.c
// * Support custom parsers?

FILE * log_file = NULL;
size_t parsed_columns_cnt = 0;
struct column * parsed_columns = NULL;

const char * workspace = NULL;
static FILE * input_file = NULL;
static FILE * output_file = NULL;
static char delimiter = '\0';
static char * header_str = NULL;
static size_t data_offset = 0;
static long row_index = 0;
static union cell * row = NULL;
static bool * row_isvalid = NULL;
static size_t grid_rows_printed = 0;
static size_t max_grid_rows = 100;
static bool stop_processing = false;
static char * line = NULL;
static size_t line_len = 0;
static size_t line_next_index = 0;
static char * line_last = NULL;

static void
setup_workspace() {
    if (workspace == NULL) {
        static char workspace_template[] = "livid-wkspace-XXXXXX";
        workspace = mkdtemp(workspace_template);
        if (workspace == NULL) ERR("mktemp failed");
    }

    LOG("Workspace: '%s'", workspace);
    int rc = chdir(workspace);
    if (rc != 0) ERR("chdir(\"%s\") failed", workspace);

    log_file = fopen("log", "w");
    if (log_file == NULL) {
        log_file = stderr;
        ERR("unable to open log file");
    }
    LOG("starting log file");

    FILE * livid_h_file = fopen("livid.h", "w");
    if (livid_h_file == NULL) ERR("unable to open livid.h");
    fwrite(src_livid_h, src_livid_h_len, 1, livid_h_file);
    fclose(livid_h_file);
}

static void
read_header() {
    size_t header_strlen = 0;
    ssize_t rc = getline(&header_str, &header_strlen, input_file);
    if (rc <= 0) ERR("unable to read header");
    if (memchr(header_str, '\0', (size_t) rc) != NULL) ERR("header has a null byte");
    header_str[rc - 1] = '\0';
    data_offset = (size_t) rc;

    // TODO: This over-allocates memory
    parsed_columns = calloc(2 + (size_t) rc, sizeof(*parsed_columns));
    if (parsed_columns == NULL) ERR("calloc failed");

    struct column * column = parsed_columns;
    struct column index_column = (struct column) {
        .name = "_index",
        .type = TYPE_LONG,
        .grid = {
            .hidden = false,
            .max_width = strlen("_index"),
        },
    };
    memcpy(column++, &index_column, sizeof *column);
    parsed_columns_cnt++;

    const char delim[2] = {delimiter, '\0'};
    char * savptr = NULL;
    for (const char * column_name = strtok_r(header_str, delim, &savptr); column_name != NULL; column_name = strtok_r(NULL, delim, &savptr)) {
        struct column new_column = (struct column) {
            .name = column_name,
            .type = TYPE_STR,
            .grid = {
                .hidden = false,
                .max_width = strlen(column_name)
            },
        };
        LOG("Parsed column #%zu: %s", parsed_columns_cnt, column_name);
        memcpy(column++, &new_column, sizeof *column);
        parsed_columns_cnt++;
    }

    row_index = 1;
    row = calloc(parsed_columns_cnt + 1, sizeof(*row));
    if (row == NULL) ERR("calloc failed");
    row_isvalid = calloc(parsed_columns_cnt + 1, sizeof(*row_isvalid));
    if (row == NULL) ERR("calloc failed");
}

bool
lv_next() {
    if (stop_processing) return false;

    line_next_index = 0;
    ssize_t rc = getline(&line, &line_len, input_file);
    if (rc <= 0) return false;
    
    return true;
}

int
lv_load(union cell * row, ssize_t column_index) {
    if (stop_processing) return -1;

    if (line_next_index == 0) {
        memset(row, 0, sizeof(*row) * parsed_columns_cnt);
        if (parsed_columns[0].type != TYPE_LONG)
            ERR("_index column must be type LONG");
        if (parsed_columns[0].index != -1) {
            row[parsed_columns[0].index].cell_long = row_index;
            row_isvalid[parsed_columns[0].index] = true;
        }
        row_index = 1;
        line_last = line;
    }
    LOG("Parsing line %zu: '%s'", row_index, line_last);

    size_t i = line_next_index;
    size_t w = 0;
    char * last = line_last;
    for (char * c = line_last; *c != '\0'; c++) {
        if (*c == '\n' || *c == delimiter) {
            //if (w > parsed_columns[i].grid.max_width) parsed_columns[i].grid.max_width = w;

            *c = '\0';
            ssize_t j = parsed_columns[i].index;
            LOG("Parsing column %zu as index %zu", i, j);
            if (j >= 0) {
                row_isvalid[j] = true;
                switch(parsed_columns[j].type) {
                case TYPE_STR:
                    row[j].cell_str = last;
                    break;
                case TYPE_LONG:
                    if (last[0] == '\0')
                        row_isvalid[j] = false;
                    row[j].cell_long = strtol(last, NULL, 0);
                    break;
                case TYPE_TIME:
                    if (last[0] == '\0')
                        row_isvalid[j] = false;
                    row[j].cell_time = strtol(last, NULL, 0);
                    break;
                case TYPE_DOUBLE:
                    if (last[0] == '\0')
                        row_isvalid[j] = false;
                    row[j].cell_double = strtod(last, NULL);
                    break;
                }
            }
            i++;
            if (i >= (size_t) column_index && column_index != -1)
                break;
            w = 0;
        } else {
            w++;
        }
    }

    return 0;
}

int
lv_printf(const char * msg, ...) {
    va_list ap;
    va_start(ap, msg);
    int rc = vfprintf(output_file, msg, ap);
    va_end(ap);
    return rc;
}

int
lv_grid(struct column * columns, size_t columns_cnt, union cell * row) {
    if (grid_rows_printed >= max_grid_rows && !stop_processing) {
        stop_processing = true;
        fprintf(output_file, "...\n");
    }
    if (stop_processing) {
        return 0;
    }
    if (grid_rows_printed == 0) {
        for (size_t i = 0; i < columns_cnt; i++) {
            if (columns[i].grid.hidden) continue;
            fputc('+', output_file);
            for (size_t j = 0; j < columns[i].grid.max_width + 2; j++)
                fputc('-', output_file);
        }
        fprintf(output_file, "+\n");
        for (size_t i = 0; i < columns_cnt; i++) {
            if (columns[i].grid.hidden) continue;
            int rc = fprintf(output_file, "| %-*s ", (int) columns[i].grid.max_width, columns[i].name);
            if (rc - 3 > (ssize_t) columns[i].grid.max_width)
                columns[i].grid.max_width = (size_t) (rc - 3);
        }
        fprintf(output_file, "|\n");
        for (size_t i = 0; i < columns_cnt; i++) {
            if (columns[i].grid.hidden) continue;
            fputc('+', output_file);
            for (size_t j = 0; j < columns[i].grid.max_width + 2; j++)
                fputc('-', output_file);
        }
        fprintf(output_file, "+\n");
    }
    for (size_t i = 0; i < columns_cnt; i++) {
        if (columns[i].grid.hidden) continue;
        /*
        if (!row_isvalid[i]) {
            fputc('|', output_file);
            for (size_t j = 0; j < columns[i].grid.max_width + 2; j++)
                fputc(' ', output_file);
            continue;
        }
        */
        int rc = 0;
        switch (columns[i].type) {
        case TYPE_STR:
            rc = fprintf(output_file, "| %-*s ", (int) columns[i].grid.max_width, row[i].cell_str);
            break;
        case TYPE_LONG:
            rc = fprintf(output_file, "| %-*ld ", (int) columns[i].grid.max_width, row[i].cell_long);
            break;
        case TYPE_TIME:
            rc = fprintf(output_file, "| %-*ld ", (int) columns[i].grid.max_width, row[i].cell_time);
            break;
        case TYPE_DOUBLE:
            rc = fprintf(output_file, "| %-*lf ", (int) columns[i].grid.max_width, row[i].cell_double);
            break;
        default:
            ERR("invalid type");
            break;
        }
        if (rc - 3 > (ssize_t) columns[i].grid.max_width)
            columns[i].grid.max_width = (size_t) (rc - 3);
    }
    fprintf(output_file, "|\n");
    grid_rows_printed++;
    if (grid_rows_printed >= max_grid_rows) {
        stop_processing = true;
        fprintf(output_file, "...\n");
    }
    return 0;
}

static void
generate_script() {
    FILE * script_file = fopen("script.c", "w");
    if (script_file == NULL) ERR("unable to create script.c for writing");

    fprintf(script_file, "#define COLUMN_LIST \\\n");
    for (size_t i = 0; i < parsed_columns_cnt; i++) {
        fprintf(script_file, "    COLUMN(%16s, %6s, %s(%zu)) \\\n",
                parsed_columns[i].name,
                strtype(parsed_columns[i].type),
                parsed_columns[i].grid.hidden ? "HIDE" : "SHOW",
                parsed_columns[i].grid.max_width);
    }
    fprintf(script_file, "\n");

    fprintf(script_file, "#include \"livid.h\"\n");

    fprintf(script_file, "\n\
int process() { \n\
    write(\"%%zu %%zu %%zu\", columns_cnt, sizeof(columns), sizeof(columns[0])); \n\
    struct row row; \n\
    while (next()) { \n\
        load_all(&row); \n\
        //write(\"a=%%s b=%%s c=%%s\", row.a, row.b, row.c); \n\
        grid(&row);\n\
    } \n\
    return 0; \n\
} \n");

    fclose(script_file);
}

static int
run() {
    int rc;
    rc = fseek(input_file, (ssize_t) data_offset, SEEK_SET);
    if (rc != 0) ERR("fseek input");
    row_index = 1;

    output_file = fopen("output", "w");
    if (output_file == NULL) ERR("unable to open output file");
    grid_rows_printed = 0;
    stop_processing = false;

    rc = system("/usr/bin/gcc -std=c99 -Wall -Wextra -Wconversion -O0 -ggdb3 -D_POSIX_C_SOURCE=201704L -I. -fPIC -shared -o liblividscript.so script.c 2> gcc.stderr > gcc.stdout");
    LOG("compilation result: %d", rc);
    if (rc != 0) {
        lv_editor_reload();
        system("cat gcc.stderr >> log");
        fseek(log_file, 0, SEEK_END);
    }

    void * libscript = dlopen("./liblividscript.so", RTLD_NOW | RTLD_LOCAL);
    if (libscript == NULL) {
        ERRX("Unable to load library (%s)", dlerror());
        return -1;
    }

    struct column * columns = dlsym(libscript, "columns");
    if (columns == NULL) {
        LOG("Unable to load 'columns' symbol (%s)", dlerror());
        goto fail;
    }
    size_t * columns_cnt = dlsym(libscript, "columns_cnt");
    if (columns == NULL) {
        LOG("Unable to load 'columns_cnt' symbol (%s)", dlerror());
        goto fail;
    }
    LOG("columns_cnt: %zu", *columns_cnt);
    for (size_t j = 0; j < parsed_columns_cnt; j++) {
        parsed_columns[j].index = -1;
    }
    for (size_t i = 0; i < *columns_cnt; i++) {
        if (strcmp(columns[i].name, "_index") == 0 && columns[i].type != TYPE_LONG) {
            LOG("column '_index' must be type LONG");
            goto fail;
        }
        columns[i].index = -1;
        for (size_t j = 0; j < parsed_columns_cnt; j++) {
            if (strcmp(columns[i].name, parsed_columns[j].name) == 0) {
                parsed_columns[j].grid.hidden = columns[i].grid.hidden;
                parsed_columns[j].type = columns[i].type;
                parsed_columns[j].index = (ssize_t) i;
                columns[i].index = (ssize_t) j;
                LOG("Mapped column %s %zd -> %zd", columns[i].name, i, j);
                continue;
            }
        }
        if (columns[i].index == -1) {
            LOG("Cannot find column named '%s'", columns[i].name);
        }
    }

    void * process_ptr = dlsym(libscript, "process");
    if (process_ptr == NULL) {
        LOG("Unable to load 'process' symbol (%s)", dlerror());
        goto fail;
    }

    int (*process_fn)(void) = process_ptr;
    rc = process_fn();
    LOG("process returned %d", rc);

    fflush(output_file);
    dlclose(libscript);
    lv_editor_reload();
    return 0;

fail:
    dlclose(libscript);
    return -1;
}

int
main(int argc, char ** argv) {
    log_file = stderr;

    optim_t * opt = optim_start(argc, argv, "[<file>]");
    if (opt == NULL) ERRX("optim_start failed");

    optim_usage(opt, "livid - vim + C interface for tabular data\n");
    optim_version(opt, "livid Version 0.1\nAuthor: Zach Banks <zjbanks@gmail.com>\n");

    optim_arg(opt, 't', "delimiter", "char",
              "Delimiter to separate columns. "
              "Must be 1 character. "
              "Defaults to ','. ");
    const char * delim_str = optim_get_string(opt, ",");
    if (strlen(delim_str) != 1) {
        optim_error(opt, "Delimiter must be 1 character, got '%s'", delim_str);
    } else {
        delimiter = delim_str[0];
    }

    optim_arg(opt, 'w', "workspace", "path",
              "Working directory to hold temporary files. "
              "Must already exist if specified. "
              "Defaults to a temporary folder prefixed with 'livid-wkspace-' if not specfied.");
    workspace = optim_get_string(opt, NULL);

    optim_usage(opt, "Defaults to read from stdin if <file> is not specified.");
    optim_positionals(opt);
    const char * input_filename = optim_get_string(opt, "/dev/stdin");
    input_file = fopen(input_filename, "r");
    if (input_file == NULL) {
        optim_error(opt, "Unable to open file '%s' for reading, error %s",
                    input_filename, strerror(errno));
    }

    int rc = optim_finish(&opt);
    if (rc < 0) return EXIT_FAILURE;
    if (rc > 0) return EXIT_SUCCESS;
    
    setup_workspace();
    read_header();
    generate_script();
    run();
    lv_editor_start("output", "script.c", "log");

    int in_fd = inotify_init1(IN_NONBLOCK);
    if (in_fd < 0) ERR("inotify_init failure");

    int in_wd = inotify_add_watch(in_fd, "script.c", IN_CLOSE_WRITE);
    if (in_wd < 0) ERR("inotify_add_watch failure");

    nfds_t nfds = 2;
    struct pollfd fds[nfds];
    fds[0].fd = in_fd;
    fds[0].events = POLLIN;
    fds[1].fd = lv_editor_waitfd();
    fds[1].events = POLLIN;

    while (1) {
        rc = poll(fds, nfds, -1);
        if (rc < 0) ERR("poll failure");
        if (rc == 0) continue;
        if (fds[0].revents & POLLIN) {
            char buf[4096];
            while (read(fds[0].fd, buf, sizeof(buf)) > 0);
            run();
        }
        if (fds[1].revents & POLLIN) {
            LOG("done");
            return EXIT_SUCCESS;
        }
    }

    return EXIT_SUCCESS;
}
