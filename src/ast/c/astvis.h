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

#endif
