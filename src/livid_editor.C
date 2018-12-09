#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include "livid_editor.h"

static int waitfd = -1;

int
lv_editor_start(
        const char * output_filename,
        const char * source_filename,
        const char * log_filename)
{
    if (waitfd >= 0) {
        LOG("lv_editor_create called while editor is already running");
        return -1;
    }
    int pipefds[2];
    int rc = pipe(pipefds);
    if (rc < 0) ERR("pipe");

    FILE * vimscript = fopen("vimrc", "w");
    if (vimscript == NULL) ERR("fopen vimrc");
    fprintf(vimscript, "set backupcopy=yes\n");
    fprintf(vimscript, "set autoread\n");
    fprintf(vimscript, "split %s\n", output_filename);
    fprintf(vimscript, "split %s\n", source_filename);
    fprintf(vimscript, "vsplit %s\n", log_filename);
    fclose(vimscript);

    pid_t pid = fork();
    if (pid == 0) {
        close(pipefds[0]);
        system("vim --servername livid -S vimrc");
        write(pipefds[1], "\0", 1);
        exit(0);
    }

    close(pipefds[1]);
    waitfd = pipefds[0];
    return 0;
}

void
lv_editor_reload(void)
{
    if (waitfd < 0)
        return;
    system("vim --servername livid --remote-send '<Esc>:checktime<CR>'");
}

int
lv_editor_waitfd(void)
{
    return waitfd;
}
