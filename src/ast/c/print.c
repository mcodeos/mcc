// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#include <stdio.h>
#include <stdarg.h>
#include <string.h>
#include <stdlib.h>
#include "common.h"

static FILE* g_log_file = NULL;

void mc_log_init(const char* log_file) {
    if (log_file != NULL) {
        g_log_file = fopen(log_file, "a");
    }
}

void mc_log_close(void) {
    if (g_log_file != NULL) {
        fclose(g_log_file);
        g_log_file = NULL;
    }
}

int mc_printf(const char* format, ...) {
    va_list args;
    char buffer[4096];
    
    va_start(args, format);
    int ret = vsnprintf(buffer, sizeof(buffer), format, args);
    va_end(args);
    
    fputs(buffer, stderr);
    fflush(stderr);
    
    if (g_log_file != NULL) {
        fputs(buffer, g_log_file);
        fflush(g_log_file);
    } else {
        char* log_file = getenv("MCC_LOG_FILE");
        if (log_file != NULL) {
            g_log_file = fopen(log_file, "a");
            if (g_log_file != NULL) {
                fputs(buffer, g_log_file);
                fflush(g_log_file);
            }
        }
    }
    
    return ret;
}
