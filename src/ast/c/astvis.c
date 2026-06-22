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
    [17] = "use",       [18] = "use_pub",

    [21] = "name",      [22] = "params_pre", [23] = "params",
    [24] = "param",     [25] = "body",     [26] = "declare",
    [27] = "declare_uv",[28] = "class",    [29] = "instance",

    [30] = "attribute", [31] = "attribute_pin",   [32] = "attribute_pinadd",
    [33] = "net",       [34] = "net_ports",      [35] = "expression",
    [36] = "role",      [37] = "enum_values",

    [38] = "iotype",    [39] = "iotype_in",      [40] = "iotype_out",
    [41] = "iotype_io", [42] = "iotype_return",  [43] = "iotype_ps",
    [44] = "iotype_anl",[45] = "iotype_nc",

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

    [101] = "uri_prefix",[102] = "uri_version",[103] = "uri_module",
    [104] = "uri_file",  [105] = "uri_asid",   [106] = "uri_import_ids",

    [111] = "set",       [112] = "set_attributes",[113] = "kvs",
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
                       COLOR_DIM, COLOR_RESET, color, name, COLOR_DIM, COLOR_RESET);
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

// Mode control
ast_visit_mode_t mcc_visit_get_mode(void) {
    return g_visit_mode;
}

void mcc_visit_set_mode(int mode) {
    if (mode < 0 || mode > 2) mode = 0;
    g_visit_mode = (ast_visit_mode_t)mode;
}
