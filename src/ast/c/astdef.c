// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#include <stdlib.h>
#include <stdbool.h> 
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <assert.h>
#include <dirent.h>
#include "lex.h"
#include "astdef.h"
#include "common.h"
#include "mca.tab.h"

//////////////////////////////////////////////////
mc_value* mc_value_create(void)
{
    mc_value* value = (mc_value*)malloc(sizeof(mc_value));
    value->type = 0;
    value->data = NULL;
    value->pos = 0;
    value->len = 0;
    value->next = NULL;
    value->sub = NULL;
    return value;
}

void mc_value_free(mc_value** value)
{
    if (value == NULL || *value == NULL)
        return;
    if ((*value)->data) {
        free((*value)->data);
        (*value)->data = NULL;
    }
    free(*value);
    *value = NULL;
}

mc_value* mc_value_create_node(unsigned short type, mc_value* sub)
{
    mc_value* value = mc_value_create();
    value->type = type;
    value->sub = sub;
    if (sub != NULL) {
        value->pos = sub->pos;
        value->len = sub->len;
    }
    return value;
}

mc_value* mc_value_create_data(unsigned short type, void* data, unsigned int pos, unsigned int len)
{
    mc_value* value = mc_value_create();
    value->type = type;
    value->data = data;
    value->pos = pos;
    value->len = len;
    return value;
}

mc_value* mc_value_link(mc_value* va, mc_value* vb)
{
    if (va != NULL && vb == NULL) return va;
    if (va == NULL && vb != NULL) return vb;
    if (va == NULL && vb == NULL) return NULL;

    mc_value* tail = va;
    while (tail->next != NULL) tail = tail->next;
    tail->next = vb;

    // va already has a position from a previous link or create_node call.
    // Since grammar rules always link nodes in source order, va->pos is the
    // minimum position. Just extend len from vb's chain.
    if (va->pos != 0) {
        unsigned int va_end = va->pos + va->len;
        for (mc_value* p = vb; p != NULL; p = p->next) {
            unsigned int end = p->pos + p->len;
            if (end > va_end) va_end = end;
        }
        va->len = va_end - va->pos;
    } else {
        // va has no position, find both pos and len from vb's chain
        unsigned int first_pos = 0;
        unsigned int last_end = 0;
        for (mc_value* p = vb; p != NULL; p = p->next) {
            if (p->pos != 0 && first_pos == 0) first_pos = p->pos;
            if (p->pos != 0) {
                unsigned int end = p->pos + p->len;
                if (end > last_end) last_end = end;
            }
        }
        if (first_pos != 0) {
            va->pos = first_pos;
            va->len = last_end - first_pos;
        }
    }

    return va;
}

mc_value* mc_value_link3(mc_value* va, mc_value* vb, mc_value* vc)
{
    return mc_value_link(mc_value_link(va, vb), vc);
}

mc_value* mc_value_link4(mc_value* va, mc_value* vb, mc_value* vc, mc_value* vd)
{
    return mc_value_link(mc_value_link3(va, vb, vc), vd);
}

//////////////////////////////////////////////////
mc_sem_token* g_sem_token_list = NULL; 
mc_sem_token* g_sem_token_list_tail = NULL; 

mc_sem_token* mcc_get_sem_tokens()
{
    return g_sem_token_list;
}

void mc_sem_token_add(short type, void* ptoken)
{
    if (ptoken == NULL) return;

    mc_sem_token* token = (mc_sem_token*) malloc(sizeof(mc_sem_token));
    token->type = type;
    token->pos = ((mc_lex_token*)ptoken)->tpos;
    token->len = ((mc_lex_token*)ptoken)->tlen;
    #ifdef DEBUG_TOKEN
    token->string = strndup(((mc_lex_token*)ptoken)->tstring, token->len);
    #endif
    token->next = NULL;
    if (g_sem_token_list_tail == NULL){
        g_sem_token_list = token;
        g_sem_token_list_tail = token;
    }else{
        g_sem_token_list_tail->next = token;
        g_sem_token_list_tail = token;
    }
}

// ── Post-lex comment scan ────────────────────────────────────────────────
// ELC ("whitespace-or-comment") is embedded in many token rules (e.g.
// LP="(" ELC, COMMA="," ELC).  When ELC consumes a //, #, or /* */ comment
// as part of another token, the comment is never emitted as a standalone
// semantic token.  This pass walks the source buffer, finds every comment
// that is NOT already covered by an existing sem token, and emits a
// MCC_TK_COMMENT sem token for it.

/// Trim any existing token whose span covers `pos` so the region
/// from `pos` onward is freed for a comment token.
static void mc_sem_trim_at(unsigned int pos) {
    for (mc_sem_token* t = g_sem_token_list; t != NULL; t = t->next) {
        if (t->pos < pos && pos < t->pos + t->len) {
            t->len = pos - t->pos;
        }
    }
}

/// Insert a sem token in position-sorted order (required by LSP delta encoding).
static void mc_sem_emit_token(short type, unsigned int pos, unsigned int len) {
    mc_sem_token* t = (mc_sem_token*) malloc(sizeof(mc_sem_token));
    t->type = type;
    t->pos = pos;
    t->len = len;
    t->next = NULL;

    // Find insertion point to keep list sorted by position
    mc_sem_token** prev = &g_sem_token_list;
    while (*prev != NULL && (*prev)->pos < pos) {
        prev = &(*prev)->next;
    }
    // Insert before the first token with larger position
    t->next = *prev;
    *prev = t;
    // Update tail if we inserted at the end
    if (t->next == NULL) {
        g_sem_token_list_tail = t;
    }
}

static bool mc_sem_has_token_at(unsigned int pos) {
    for (mc_sem_token* t = g_sem_token_list; t != NULL; t = t->next) {
        if (t->pos <= pos && pos < t->pos + t->len) return true;
    }
    return false;
}

static void mc_sem_scan_comments(const char* source, size_t source_len) {
    if (source == NULL || source_len == 0) return;
    int _found = 0, _skipped = 0;
    (void)_found; (void)_skipped;
    size_t i = 0;
    while (i + 1 < source_len) {
        // Skip string literals (don't match comment markers inside "...")
        if (source[i] == '"') {
            i++;
            while (i < source_len && source[i] != '"') {
                if (source[i] == '\\') i++;
                if (i < source_len) i++;
            }
            if (i < source_len) i++;
            continue;
        }
        // Single-line: //
        if (source[i] == '/' && i + 1 < source_len && source[i + 1] == '/') {
            unsigned int start = (unsigned int)i;
            i += 2;
            while (i < source_len && source[i] != '\n' && source[i] != '\r' && source[i] != '\0') i++;
            // ★ Trim any token whose span extends into this comment,
            //    then emit the comment token (even if overlapping).
            mc_sem_trim_at(start);
            mc_sem_emit_token(MCC_TK_COMMENT, start, (unsigned int)i - start);
            continue;
        }
        // Single-line: #
        if (source[i] == '#') {
            unsigned int start = (unsigned int)i;
            i++;
            while (i < source_len && source[i] != '\n' && source[i] != '\r' && source[i] != '\0') i++;
            mc_sem_trim_at(start);
            mc_sem_emit_token(MCC_TK_COMMENT, start, (unsigned int)i - start);
            continue;
        }
        // Multi-line: /* */
        if (source[i] == '/' && i + 1 < source_len && source[i + 1] == '*') {
            unsigned int start = (unsigned int)i;
            i += 2;
            while (i + 1 < source_len) {
                if (source[i] == '*' && source[i + 1] == '/') { i += 2; break; }
                if (source[i] == '\0') break;
                i++;
            }
            mc_sem_trim_at(start);
            mc_sem_emit_token(MCC_TK_COMMENT, start, (unsigned int)i - start);
            continue;
        }
        i++;
    }
    // [comment-scan] diagnostics available via MCC_LOG_SEM flag
}

void mc_sem_scan_and_add_comments(const char* source, size_t source_len) {
    mc_sem_scan_comments(source, source_len);
}

// Minimal compatibility version: original parameters unchanged, fix crash, dest only allowed to pass NULL
char* string_cat(char *dest, const char *src, bool withdot)
{
    const char *safe_dest = (dest == NULL) ? "" : dest;
    const char *safe_src  = (src  == NULL) ? "" : src;

    size_t len_dest = strlen(safe_dest);
    size_t len_src  = strlen(safe_src);
    size_t dot_len = (withdot && len_dest > 0 && len_src > 0) ? 1 : 0;
    size_t total_len = len_dest + dot_len + len_src + 1;

    char *result = (char *)malloc(total_len);
    if (result == NULL) {
        perror("malloc failed");
        return NULL;
    }

    char *p = result;
    memcpy(p, safe_dest, len_dest);
    p += len_dest;
    if (dot_len > 0) *p++ = '.';
    memcpy(p, safe_src, len_src + 1);

    if (dest != NULL) {
        mprintf(MCC_LOG_ERROR, "warning: dest is not NULL, skip free to avoid crash\n");
    }

    return result;
}

char* mcvalue_to_string(mc_value* pval)
{
    mc_value* value = pval;
    char* string = NULL;

    while (value != NULL)
    {
        switch (value->type)
        {
        case MCAST_ID: 
        case MCAST_IDA:
        case MCAST_INT:
            string = string_cat(string, (char*)value->data, false);
            break;

        case MCAST_OPD_IDAN:
            break;
        }
        value = value->next;
    }
    return string;
}

void mc_sem_token_add_mcvalue(short type, mc_value* pval)
{
    if (pval == NULL) return;

    char* string = mcvalue_to_string(pval);
    if(string) {
        mc_sem_token* token = (mc_sem_token*) malloc(sizeof(mc_sem_token));
        token->type = type;
        #ifdef DEBUG_TOKEN
        token->string = string;
        #endif
        token->pos = pval->pos;
        token->len = strlen(string);
        token->next = NULL;
        if (g_sem_token_list_tail == NULL){
            g_sem_token_list = token;
            g_sem_token_list_tail = token;
        }else{
            g_sem_token_list_tail->next = token;
            g_sem_token_list_tail = token;
        }
    }
}

void mc_sem_token_print()
{
    mprintf(MCC_LOG_SEM, "------------------------------------------------------------\n[sem tokens]\n");
    mc_sem_token* this = g_sem_token_list;
    while (this != NULL){
        #ifdef DEBUG_TOKEN
        mprintf(MCC_LOG_SEM, "[%d..%d|^%d|%s] ", this->pos, this->pos+this->len, this->type, this->string);
        #else
        mprintf(MCC_LOG_SEM, "[%d..%d|^%d] ", this->pos, this->pos+this->len, this->type);
        #endif
        this = this->next;
    }
}

void mc_sem_token_free()
{
    while (g_sem_token_list != NULL){
        mc_sem_token* this = g_sem_token_list;
        g_sem_token_list = g_sem_token_list->next;
        #ifdef DEBUG_TOKEN
        free(this->string);
        #endif
        free(this);
    }
    g_sem_token_list = NULL;
    g_sem_token_list_tail = NULL;
}

//////////////////////////////////////////////////
mc_sem_token* g_error_token_list = NULL;
mc_sem_token* g_error_token_list_tail = NULL;

// ── Structured diagnostic log entries ──────────────────────────────────
mc_dlog_entry* g_dlog_entries = NULL;
mc_dlog_entry* g_dlog_entries_tail = NULL;

mc_sem_token* mcc_get_error_tokens()
{
    return g_error_token_list;
}
void mc_error_token_add(void* ptoken)
{
    if (ptoken == NULL) return;

    mc_sem_token* token = (mc_sem_token*) malloc(sizeof(mc_sem_token));
    token->type = 0;
    token->pos = ((mc_lex_token*)ptoken)->tpos;
    token->len = ((mc_lex_token*)ptoken)->tlen;
    #ifdef DEBUG_TOKEN
    token->string = strndup(((mc_lex_token*)ptoken)->tstring, token->len);
    #endif
    token->next = NULL;
    if (g_error_token_list_tail == NULL){
        g_error_token_list = token;
        g_error_token_list_tail = token;
    }else{
        g_error_token_list_tail->next = token;
        g_error_token_list_tail = token;
    }
}
void mcc_clear_error_tokens()
{
    mc_sem_token* current = g_error_token_list;
    while (current != NULL) {
        mc_sem_token* next = current->next;
        free(current);
        current = next;
    }
    g_error_token_list = NULL;
    g_error_token_list_tail = NULL;
}
void mc_error_token_print()
{
    mprintf(MCC_LOG_ERROR, "\n------------------------------------------------------------\n[error tokens]\n");
    mc_sem_token* this = g_error_token_list;
    while (this != NULL){
        #ifdef DEBUG_TOKEN
        mprintf(MCC_LOG_ERROR, "[%d..%d|^%d|%s] ", this->pos, this->pos+this->len, this->type, this->string);
        #else
        mprintf(MCC_LOG_ERROR, "[%d..%d|^%d] ", this->pos, this->pos+this->len, this->type);
        #endif
        this = this->next;
    }
}
void mc_error_token_free()
{
    while (g_error_token_list != NULL){
        mc_sem_token* this = g_error_token_list;
        g_error_token_list = g_error_token_list->next;
        #ifdef DEBUG_TOKEN
        free(this->string);
        #endif
        free(this);
    }
    g_error_token_list = NULL;
    g_error_token_list_tail = NULL;
}

// ── Structured diagnostic log entry functions ──────────────────────────

void mc_dlog_add(unsigned int code, int level, unsigned int pos, unsigned int len, const char* msg)
{
    mc_dlog_entry* entry = (mc_dlog_entry*) malloc(sizeof(mc_dlog_entry));
    entry->code = code;
    entry->level = level;
    entry->pos = pos;
    entry->len = len;
    entry->msg = msg ? strdup(msg) : NULL;
    entry->next = NULL;

    if (g_dlog_entries_tail == NULL) {
        g_dlog_entries = entry;
        g_dlog_entries_tail = entry;
    } else {
        g_dlog_entries_tail->next = entry;
        g_dlog_entries_tail = entry;
    }

    mprintf(MCC_LOG_ERROR, "\n[DLOG %s%u at %u..%u] %s\n",
            level == 1 ? "E" : level == 2 ? "W" : level == 3 ? "I" : "H",
            code, pos, pos + len, msg ? msg : "");
}

mc_dlog_entry* mcc_get_dlog_entries(void)
{
    return g_dlog_entries;
}

void mc_dlog_entries_free(void)
{
    while (g_dlog_entries != NULL) {
        mc_dlog_entry* this = g_dlog_entries;
        g_dlog_entries = g_dlog_entries->next;
        if (this->msg) free(this->msg);
        free(this);
    }
    g_dlog_entries = NULL;
    g_dlog_entries_tail = NULL;
}

void mcc_clear_dlog_entries(void)
{
    mc_dlog_entries_free();
}

//////////////////////////////////////////////////
void mcc_reset(unsigned char log_flags)
{
    g_mcc_log_flags = log_flags;
    if (g_token_head) {
        free_tokens(g_token_head);
        g_token_head = NULL;
    }
    g_last_token = NULL;
    g_current_token = NULL;
    mc_sem_token_free();
    mc_error_token_free();
    mc_dlog_entries_free();
}

void mcc_lex(char* data)
{
    if (!data) return;
    mc_lex_token* m_tokens_tail = NULL;
    lex(&m_tokens_tail, data);
    print_tokens(g_token_head);

    // Post-lex: emit sem tokens for comments consumed by ELC
    mc_sem_scan_and_add_comments(data, strlen(data));
}

mc_value* mcc_parse()
{
    if (!g_token_head) return NULL;
    g_current_token = g_token_head;

    mc_value* ast_root = mc_value_create();
    // mca_debug = 1;
    int parse_result = mca_parse(ast_root);
    if (parse_result != 0) {
        mc_value_free(&ast_root);
    }
    return ast_root;
}

void mc_visit_free(mc_value* value)
{
    while (value != NULL) {
        if (value->data) {
            free(value->data);
            value->data = NULL;
        }
        if (value->sub) {
            mc_visit_free(value->sub);
        }
        mc_value* next = value->next;
        free(value);
        value = next;
    }
}

void mcc_free(mc_value* ast)
{
    mc_visit_free(ast);
}

void check_to_print_endl(mc_value* value)
{
    if (value->type == MCAST_USE || value->type == MCAST_USE_PUB ||               
        value->type == MCAST_COMPONENT ||         
        value->type == MCAST_MODULE ||            
        value->type == MCAST_INTERFACE ||         
        value->type == MCAST_FUNCTION ||          
        value->type == MCAST_ENUM ||              
        value->type == MCAST_DEFINE ||              
        value->type == MCAST_BODY ||              
        value->type == MCAST_NET ||               
        value->type == MCAST_ATTRIBUTE ||         
        value->type == MCAST_ATTRIBUTE_PIN ||     
        value->type == MCAST_ATTRIBUTE_PINADD ||  
        value->type == MCAST_PIN_LINE ||
        value->type == MCAST_COND_IF ||
        value->type == MCAST_COND_ELSE
    )
    mprintf(MCC_LOG_VISIT, "\n");
}

void mc_visit(mc_value* value)
{
    if (value == NULL) {
        mprintf(MCC_LOG_VISIT, "NULL");
        return;
    }

    while (value != NULL)
    {
        check_to_print_endl(value);

        if (value->type == MCAST_COMPONENT ||
            value->type == MCAST_MODULE ||
            value->type == MCAST_INTERFACE)
        mprintf(MCC_LOG_VISIT, "\n\n");

        if (value->data != NULL)
        {
            mprintf(MCC_LOG_VISIT, " %d:%u:%u|", value->type, value->pos, value->len);
            mprintf(MCC_LOG_VISIT, "%s", (char*)value->data);
        }
        if (value->sub != NULL)
        {
            mprintf(MCC_LOG_VISIT, " [%d:%u:%u|", value->type, value->pos, value->len);
            mc_visit(value->sub);
            mprintf(MCC_LOG_VISIT, " '%d]", value->type);
        }

        check_to_print_endl(value);

        if (value->next != NULL) mprintf(MCC_LOG_VISIT, " ");
        value = value->next;
    }
}
void mcc_visit(mc_value* ast)
{
    mprintf(MCC_LOG_VISIT, "\n------------------------------------------------------------\n[visit]");
    mc_visit(ast);
    mprintf(MCC_LOG_VISIT, "\n\n");
}
