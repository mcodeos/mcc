// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include "common.h"

unsigned char g_mcc_log_flags = MCC_LOG_NONE;

char* mcc_load(char* file) {
    FILE* fp;
    fp = fopen(file, "rb");  
    mprintf(MCC_LOG_TOKEN, "\n\n------------------------------------------------------------\n");
    mprintf(MCC_LOG_TOKEN, "open %s", file);
    
    if (!fp) {
        mprintf(MCC_LOG_TOKEN, " - FAILED: Cannot open file\n");
        return NULL;
    }
    
    fseek(fp, 0, SEEK_END);
    long size = ftell(fp);  
    mprintf(MCC_LOG_TOKEN, " file.length=%ld\n\n", size);
    
    if (size <= 0) {
        mprintf(MCC_LOG_TOKEN, " - FAILED: File is empty or error getting size\n");
        fclose(fp);
        return NULL;
    }
    
    // Allocate size + 1 bytes to ensure space for null terminator
    char* buffer = (char*)malloc(size + 2);
    if (!buffer) {
        mprintf(MCC_LOG_TOKEN, " - FAILED: Memory allocation failed\n");
        fclose(fp);
        return NULL;
    }
    
    fseek(fp, 0, SEEK_SET);
    
    // Correct use of fread: read size elements, each 1 byte
    size_t bytes_read = fread(buffer, 1, size, fp);

    if (bytes_read != (size_t)size) {
        mprintf(MCC_LOG_TOKEN, " - WARNING: Expected %ld bytes, but read %zu bytes\n", size, bytes_read);
        // Continue processing but use the actual number of bytes read
    }

    // Add null terminator
    buffer[bytes_read] = '\0';
    buffer[bytes_read+1] = '\0';
    
    fclose(fp);
    
    return buffer;
}

char* mcc_load_from_string(const char* content, size_t len) {
    mprintf(MCC_LOG_TOKEN, "\n\n------------------------------------------------------------\n");
    mprintf(MCC_LOG_TOKEN, "load from memory: %zu bytes\n\n", len);
    
    if (!content || len == 0) {
        mprintf(MCC_LOG_TOKEN, " - FAILED: Empty content\n");
        return NULL;
    }
    
    char* buffer = (char*)malloc(len + 2);
    if (!buffer) {
        mprintf(MCC_LOG_TOKEN, " - FAILED: Memory allocation failed\n");
        return NULL;
    }
    
    memcpy(buffer, content, len);
    buffer[len] = '\0';
    buffer[len + 1] = '\0';
    
    return buffer;
}