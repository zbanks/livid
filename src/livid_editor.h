#ifndef __LIVID_EDITOR_H__
#define __LIVID_EDITOR_H__

int lv_editor_start(const char * output_filename, const char * source_filename, const char * log_filename);
void lv_editor_reload(void);
int lv_editor_waitfd(void);

#endif
