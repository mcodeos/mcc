// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#ifndef _ASTVIS_H_
#define _ASTVIS_H_

#include "astdef.h"

// AST visitor output mode
typedef enum {
    AST_VISIT_FLAT,      // Original flat format
    AST_VISIT_TREE,      // Indented tree
    AST_VISIT_TREE_COLOR // Color indented tree
} ast_visit_mode_t;

// Get the current output mode
ast_visit_mode_t mcc_visit_get_mode(void);

// Set the output mode (0=flat, 1=tree, 2=color)
void mcc_visit_set_mode(int mode);

// Tree-print AST
void mcc_visit_tree(mc_value* ast);

// Color tree-print AST
void mcc_visit_tree_color(mc_value* ast);

// Build a JSON rendering of the AST into an internal buffer (no stdout).
// Read it back with mcc_visit_json_data()/mcc_visit_json_len() and release
// it with mcc_visit_json_free().
void mcc_visit_tree_json(mc_value* ast);
const char* mcc_visit_json_data(void);
int mcc_visit_json_len(void);
void mcc_visit_json_free(void);

#endif
