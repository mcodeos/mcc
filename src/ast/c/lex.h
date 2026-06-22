// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#ifndef __LEX_H__
#define __LEX_H__

#include <stdint.h>

// Forward declaration for YYLTYPE (defined by Bison)
struct YYLTYPE;
typedef struct _mc_lex_token
{
    short tid;
    char* tstring;
    unsigned int tpos;
    unsigned int tlen;
    uint8_t* next;
} mc_lex_token;

void lex(mc_lex_token** tokens, char* s);
int mca_lex(void *yylval_param, struct YYLTYPE *yylloc_param);
void add_token(mc_lex_token** p_tokens_tail, short class, short id, char* string, unsigned int pos, unsigned short len);
void free_tokens(mc_lex_token* tokens);
void print_tokens(mc_lex_token* tokens);

extern mc_lex_token* g_token_head;
extern mc_lex_token* g_current_token;
extern mc_lex_token* g_last_token;

#endif /* __LEX_H__ */
