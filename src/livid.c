#include <dlfcn.h>
#include <errno.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "optim.h"

#define LOG(msg, ...) fprintf(stderr, __FILE__ ":%d:%s " msg "\n", __LINE__, __func__, ## __VA_ARGS__);
#define ERRX(msg, ...) ({ \
        LOG(msg, ## __VA_ARGS__); \
        exit(EXIT_FAILURE); \
    })
#define ERR(msg, ...) ERRX(msg " (%s)", ## __VA_ARGS__, strerror(errno))

const char * workspace = NULL;
FILE * input_file = NULL;
FILE * output_file = NULL;
char delimiter = '\0';
char * header_str = NULL;
char ** row = NULL;
size_t fields_cnt = 0;
struct field {
    const char * name;
    size_t index;
    enum field_type {
        TYPE_STR,
        TYPE_TIME,
        TYPE_LONG,
        TYPE_DOUBLE,
    } type;
    struct grid {
        bool hidden;
        size_t max_width;
    } grid;
} * fields = NULL;

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

    output_file = fopen("output", "w");
    if (output_file == NULL) ERR("unable to open output file");
}

static void
read_header() {
    size_t header_strlen = 0;
    ssize_t rc = getline(&header_str, &header_strlen, input_file);
    if (rc <= 0) ERR("unable to read header");
    if (memchr(header_str, '\0', (size_t) rc) != NULL) ERR("header has a null byte");
    header_str[rc - 1] = '\0';

    // TODO: This over-allocates memory
    fields = calloc((size_t) rc, sizeof(*fields));
    if (fields == NULL) ERR("calloc failed");

    struct field * field = fields;
    const char delim[2] = {delimiter, '\0'};
    char * savptr = NULL;
    for (const char * field_name = strtok_r(header_str, delim, &savptr); field_name != NULL; field_name = strtok_r(NULL, delim, &savptr)) {
        field->name = field_name;
        field->index = fields_cnt;
        LOG("Parsed field #%zu: %s", field->index, field->name);
        field++;
        fields_cnt++;
    }

    row = calloc(fields_cnt + 1, sizeof(*row));
    if (row == NULL) ERR("calloc failed");
}

void *
lv_next() {
    static char * line = NULL;
    static size_t line_len = 0;
    ssize_t rc = getline(&line, &line_len, input_file);
    if (rc < 0) return NULL;

    memset(row, 0, sizeof(*row) * fields_cnt);
    size_t i = 0;
    size_t w = 0;
    char * last = line;
    for (char * c = line; *c != '\0'; c++) {
        if (*c == '\n' || *c == delimiter) {
            if (w > fields[i].grid.max_width)
                fields[i].grid.max_width = w;

            *c = '\0';
            row[i++] = last;
            w = 0;
        } else {
            w++;
        }
    }

    return row;
}

int
lv_printf(const char * msg, ...) {
    va_list ap;
    va_start(ap, msg);
    int rc = vfprintf(output_file, msg, ap);
    va_end(ap);
    return rc;
}

static void
generate_script() {
    FILE * script_file = fopen("script.c", "w");
    if (script_file == NULL) ERR("unable to create script.c for writing");

    fprintf(script_file, "#include <stdbool.h>\n");
    fprintf(script_file, "#include <stddef.h>\n");
    fprintf(script_file, "#include <stdint.h>\n");
    fprintf(script_file, "#include <stdio.h>\n");
    fprintf(script_file, "#include <stdlib.h>\n");

    fprintf(script_file, "struct row {\n");
    for (size_t i = 0; i < fields_cnt; i++) {
        fprintf(script_file, "    char * %s;\n", fields[i].name);
    }
    fprintf(script_file, "};\n\n");
    fprintf(script_file, "void * lv_next(void);\n");
    fprintf(script_file, "#define next() ((struct row *) lv_next())\n\n");
    fprintf(script_file, "int lv_write(const char * msg, ...);\n");
    fprintf(script_file, "#define write lv_printf\n\n");

    fprintf(script_file, "\n\
int process() { \n\
    struct row * row = NULL; \n\
    while ((row = next())) { \n\
        write(\"a=%%s b=%%s c=%%s\\n\", row->a, row->b, row->c); \n\
    } \n\
    return 0; \n\
} \n");

    fclose(script_file);
}

static int
run() {
    int rc = system("/usr/bin/gcc -std=c99 -Wall -Wextra -Wconversion -O0 -ggdb3 -D_POSIX_C_SOURCE=201704L -I. -fPIC -shared -o liblividscript.so script.c 2> gcc.stderr > gcc.stdout");
    LOG("compilation result: %d", rc);
    if (rc != 0) return rc;

    void * libscript = dlopen("./liblividscript.so", RTLD_NOW | RTLD_LOCAL);
    if (libscript == NULL) {
        ERRX("Unable to load library (%s)", dlerror());
        return -1;
    }

    void * process_ptr = dlsym(libscript, "process");
    if (process_ptr == NULL) {
        ERRX("Unable to load 'process' symbol (%s)", dlerror());
        goto fail;
    }

    int (*process_fn)(void) = process_ptr;
    rc = process_fn();
    LOG("process returned %d", rc);

    fflush(output_file);

fail:
    dlclose(libscript);
    return -1;
}

static void
open_editor() {
    system("vim -O script.c output");
}

int
main(int argc, char ** argv) {
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
    open_editor();

    return 0;
}
