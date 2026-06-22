// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#ifndef __MC_BASE_H__
#define __MC_BASE_H__

char* mcc_load(char* file);
char* mcc_load_from_string(const char* content, size_t len);
char* mc_strcat(char* str1, char* str2);
int mc_printf(const char *format,...);
void mc_log_init(const char* log_file);
void mc_log_close(void);

//---------------------------
// AST log categories (bitflags for mcc_reset)
#define MCC_LOG_NONE   0
#define MCC_LOG_TOKEN   (1 << 0)   // print_tokens: lexer output
#define MCC_LOG_SEM     (1 << 1)   // mc_sem_token_print: semantic tokens
#define MCC_LOG_AST     (1 << 2)   // mca.y grammar actions
#define MCC_LOG_VISIT   (1 << 3)   // mcc_visit: AST visit output
#define MCC_LOG_ERROR   (1 << 4)   // mc_error_token_print: error tokens
#define MCC_LOG_ALL     0xFF       // enable all logs

extern unsigned char g_mcc_log_flags;

// mprintf: output to log file (mc_printf)
#define mprintf(flag, ...) do { \
    if (g_mcc_log_flags & (flag)) { \
        mc_printf(__VA_ARGS__); \
    } \
} while(0)

#endif
