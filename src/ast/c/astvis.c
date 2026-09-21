// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "astvis.h"
#include "common.h"

// Current output mode
static ast_visit_mode_t g_visit_mode = AST_VISIT_TREE_COLOR;

// ANSI color codes
#define COLOR_RESET   "\033[0m"
#define COLOR_BOLD    "\033[1m"
#define COLOR_DIM     "\033[2m"

// Type colors
#define COLOR_MODULE    "\033[36m"  // cyan - module
#define COLOR_COMPONENT "\033[33m"  // yellow - component
#define COLOR_INTERFACE "\033[35m"  // purple - interface
#define COLOR_NET       "\033[32m"  // green - net
#define COLOR_PIN       "\033[34m"  // blue - pin
#define COLOR_PARAM     "\033[33m"  // yellow - param
#define COLOR_INSTANCE  "\033[96m"  // bright cyan - instance
#define COLOR_USE       "\033[94m"  // blue - use statement
#define COLOR_EXPR      "\033[37m"  // white - expression
#define COLOR_ATTR      "\033[90m"  // gray - attribute
#define COLOR_ID        "\033[37m"  // white - identifier
#define COLOR_NUM       "\033[93m"  // bright yellow - number
#define COLOR_DEFAULT   "\033[90m"  // gray - default

// Type name mapping
static const char* type_names[] = {
    [0]  = "unknown",
    [1]  = "id",       [2]  = "ida",      [3]  = "ids",
    [4]  = "int",      [5]  = "hex",      [6]  = "float",
    [7]  = "string",   [8]  = "const",    [9]  = "uvalue",

    [11] = "component", [12] = "module",   [13] = "interface",
    [14] = "function",  [15] = "enum",     [16] = "define",
    [17] = "use",       [18] = "use_pub",  [19] = "capability",

    [21] = "name",      [22] = "params_pre", [23] = "params",
    [24] = "param",     [25] = "body",     [26] = "declare",
    [27] = "declare_uv",[28] = "class",    [29] = "instance",

    [30] = "attribute", [31] = "attribute_pin",   [32] = "attribute_pinadd",
    [33] = "net",       [34] = "net_ports",      [35] = "expression",
    [36] = "role",      [37] = "enum_values",

    [38] = "iotype",    [39] = "iotype_in",      [40] = "iotype_out",
    [41] = "iotype_io", [42] = "iotype_return",  [43] = "iotype_ps",
    [45] = "iotype_nc",

    [46] = "att_id",    [47] = "att_values",
    [48] = "pin_line",  [49] = "pin_id",    [50] = "pin_names",
    [51] = "pin_name",

    [52] = "opd",       [53] = "opd_this",   [54] = "opd_pins",
    [55] = "opd_member",[56] = "opd_idan",   [57] = "opd_uscore",
    [58] = "opd_nc",

    [59] = "opd_dot",   [60] = "opd_curly",  [61] = "opd_curly_mn",
    [62] = "opd_square_vec",[63] = "opr_paren",
    [64] = "opd_apost", [65] = "opd_caret",

    [66] = "opd_plus",  [67] = "opd_minus",   [68] = "opd_multi",
    [69] = "opd_divid", [70] = "opd_leftarrow",[71] = "opd_rightarrow",

    [72] = "opd_fcall", [73] = "opd_closure",[74] = "opd_group",
    [75] = "opd_tilde", [76] = "opd_colon",  [77] = "opd_dbcolon",

    [81] = "cond_if",   [82] = "cond_else",   [83] = "cond_block",
    [84] = "judge_eqeq",[85] = "judge_noteq",[86] = "judge_lessthan",
    [87] = "judge_greaterthan",[88] = "judge_lesseqthan",
    [89] = "judge_greatereqthan",[90] = "judge_bitand",
    [91] = "judge_bitor",[92] = "judge_in",
    [94] = "abstract",   [95] = "variant",  [96] = "adopts",

    [101] = "uri_prefix",[102] = "uri_version",[103] = "uri_module",
    [104] = "uri_file",  [105] = "uri_asid",   [106] = "uri_import_ids",

    [111] = "set",       [112] = "set_attributes",
};

// Get type name
const char* mcc_type_name(int type) {
    if (type >= 0 && type < (int)(sizeof(type_names)/sizeof(type_names[0]))) {
        if (type_names[type] != NULL) {
            return type_names[type];
        }
    }
    static char buf[32];
    snprintf(buf, sizeof(buf), "TYPE_%d", type);
    return buf;
}

// Color by depth (depth 0 = top level)
static const char* depth_color(int depth) {
    // Rainbow color scheme, from light to dark
    switch (depth % 8) {
        case 0: return "\033[94m";  // blue - top level (module, component...)
        case 1: return "\033[36m";  // cyan - second level
        case 2: return "\033[32m";  // green
        case 3: return "\033[33m";  // yellow
        case 4: return "\033[35m";  // purple
        case 5: return "\033[31m";  // red
        case 6: return "\033[96m";  // bright cyan
        case 7: return "\033[92m";  // bright green
        default: return "\033[37m";  // white
    }
}

// Indent print
static void print_indent(int depth) {
    for (int i = 0; i < depth; i++) {
        printf("│   ");
    }
    printf("├── ");
}

// Tree-print core
static void visit_tree_recursive(mc_value* node, int depth) {
    while (node != NULL) {
        // Skip empty nodes with type == 0
        if (node->type != 0) {
            print_indent(depth);

            const char* name = mcc_type_name(node->type);
            const char* data = (char*)node->data;

            if (data != NULL && data[0] != '\0') {
                printf("[%s] ", name);
                fwrite(data, 1, strlen(data), stdout);
                printf("\n");
            } else {
                printf("[%s]\n", name);
            }
            fflush(stdout);

            // Limit recursion depth to prevent stack overflow
            if (depth < 100 && node->sub != NULL) {
                visit_tree_recursive(node->sub, depth + 1);
            }
        }
        
        node = node->next;
    }
}

void mcc_visit_tree(mc_value* ast) {
    printf("\n");
    printf("╔══════════════════════════════════════════════════════════════╗\n");
    printf("║                       AST TREE                               ║\n");
    printf("╚══════════════════════════════════════════════════════════════╝\n");
    printf("\n");
    
    if (ast == NULL) {
        printf("  (empty)\n");
        return;
    }
    
    visit_tree_recursive(ast, 0);
    printf("\n");
}

// Color tree-print core - color by depth
static void visit_tree_color_recursive(mc_value* node, int depth) {
    while (node != NULL) {
        // Skip empty nodes with type == 0
        if (node->type != 0) {
            print_indent(depth);

            const char* name = mcc_type_name(node->type);
            const char* color = depth_color(depth);  // color by depth
            const char* data = (char*)node->data;

            if (data != NULL && data[0] != '\0') {
                printf("%s[%s%s%s]%s ",
                       COLOR_DIM, COLOR_RESET, color, name, COLOR_DIM);
                fwrite(data, 1, strlen(data), stdout);
                printf(COLOR_RESET "\n");
            } else {
                printf("%s[%s%s%s]%s\n",
                       COLOR_DIM, COLOR_RESET, color, name, COLOR_RESET);
            }
            fflush(stdout);

            // Limit recursion depth to prevent stack overflow
            if (depth < 100 && node->sub != NULL) {
                visit_tree_color_recursive(node->sub, depth + 1);
            }
        }
        
        node = node->next;
    }
}

void mcc_visit_tree_color(mc_value* ast) {
    printf("\n");
    printf(COLOR_BOLD "╔══════════════════════════════════════════════════════════════╗\n");
    printf("║                       AST TREE                               ║\n");
    printf("╚══════════════════════════════════════════════════════════════╝\n" COLOR_RESET);
    printf("\n");

    if (ast == NULL) {
        printf("  (empty)\n" COLOR_RESET);
        fflush(stdout);
        return;
    }

    visit_tree_color_recursive(ast, 0);
    // Ensure default color is restored on exit to avoid color bleeding into subsequent terminal output
    printf("\n" COLOR_RESET);
    fflush(stdout);
}

// JSON tree capture: a growable buffer the json visitor fills so the host
// can read the rendering back without routing it through stdout.
static char* g_json_buf = NULL;
static size_t g_json_len = 0;
static size_t g_json_cap = 0;

static int json_buf_append(const char* s, size_t n) {
    if (g_json_len + n + 1 > g_json_cap) {
        size_t cap = g_json_cap ? g_json_cap : 4096;
        while (g_json_len + n + 1 > cap) cap *= 2;
        char* buf = realloc(g_json_buf, cap);
        if (buf == NULL) return 0;
        g_json_buf = buf;
        g_json_cap = cap;
    }
    memcpy(g_json_buf + g_json_len, s, n);
    g_json_len += n;
    g_json_buf[g_json_len] = '\0';
    return 1;
}

static int json_buf_str(const char* s) {
    return json_buf_append(s, strlen(s));
}

// Append s as a JSON string literal; control bytes are escaped, other bytes
// (including UTF-8 source text) pass through untouched.
static int json_buf_escaped(const char* s) {
    if (!json_buf_str("\"")) return 0;
    for (const unsigned char* p = (const unsigned char*)s; *p != '\0'; p++) {
        char esc[8];
        switch (*p) {
            case '"':  if (!json_buf_str("\\\"")) return 0; break;
            case '\\': if (!json_buf_str("\\\\")) return 0; break;
            case '\b': if (!json_buf_str("\\b")) return 0; break;
            case '\f': if (!json_buf_str("\\f")) return 0; break;
            case '\n': if (!json_buf_str("\\n")) return 0; break;
            case '\r': if (!json_buf_str("\\r")) return 0; break;
            case '\t': if (!json_buf_str("\\t")) return 0; break;
            default:
                if (*p < 0x20) {
                    snprintf(esc, sizeof(esc), "\\u%04x", *p);
                    if (!json_buf_str(esc)) return 0;
                } else {
                    if (!json_buf_append((const char*)p, 1)) return 0;
                }
        }
    }
    return json_buf_str("\"");
}

static int json_visit_nodes(mc_value* node, int depth) {
    int first = 1;
    while (node != NULL) {
        if (node->type != 0) {
            if (!first && !json_buf_str(",")) return 0;
            first = 0;
            if (!json_buf_str("{\"kind\":")) return 0;
            if (!json_buf_escaped(mcc_type_name(node->type))) return 0;
            // Source span in bytes ([start, end) of this node), following the
            // `span: {start, end}` contract the Rust read faces render as
            // `@start:end` (output/compact.rs span_suffix). The rule span
            // (rpos/rlen, whole reduction, discarded tokens included) is the
            // coverage face; nodes without one fall back to the first-subnode
            // anchor pos/len.
            {
                char num[32];
                unsigned int s = node->pos;
                unsigned int e = node->pos + node->len;
                if (node->rlen != 0) {
                    s = node->rpos;
                    e = node->rpos + node->rlen;
                }
                if (!json_buf_str(",\"span\":{\"start\":")) return 0;
                snprintf(num, sizeof(num), "%u", s);
                if (!json_buf_str(num)) return 0;
                if (!json_buf_str(",\"end\":")) return 0;
                snprintf(num, sizeof(num), "%u", e);
                if (!json_buf_str(num)) return 0;
                if (!json_buf_str("}")) return 0;
            }
            const char* data = (const char*)node->data;
            if (data != NULL && data[0] != '\0') {
                if (!json_buf_str(",\"value\":")) return 0;
                if (!json_buf_escaped(data)) return 0;
            }
            if (depth < 100 && node->sub != NULL) {
                if (!json_buf_str(",\"children\":[")) return 0;
                if (!json_visit_nodes(node->sub, depth + 1)) return 0;
                if (!json_buf_str("]")) return 0;
            }
            if (!json_buf_str("}")) return 0;
        }
        node = node->next;
    }
    return 1;
}

void mcc_visit_tree_json(mc_value* ast) {
    g_json_len = 0;
    if (g_json_buf != NULL) g_json_buf[0] = '\0';
    if (!json_buf_str("[")) return;
    if (ast != NULL) {
        json_visit_nodes(ast, 0);
    }
    json_buf_str("]");
}

const char* mcc_visit_json_data(void) {
    return g_json_buf != NULL ? g_json_buf : "[]";
}

int mcc_visit_json_len(void) {
    return (int)g_json_len;
}

void mcc_visit_json_free(void) {
    free(g_json_buf);
    g_json_buf = NULL;
    g_json_len = 0;
    g_json_cap = 0;
}

// Mode control
ast_visit_mode_t mcc_visit_get_mode(void) {
    return g_visit_mode;
}

void mcc_visit_set_mode(int mode) {
    if (mode < 0 || mode > 2) mode = 0;
    g_visit_mode = (ast_visit_mode_t)mode;
}
