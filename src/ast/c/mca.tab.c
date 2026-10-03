/* A Bison parser, made by GNU Bison 3.8.2.  */
/* Skeleton implementation for Bison GLR parsers in C
   Copyright (C) 2002-2015, 2018-2021 Free Software Foundation, Inc.
   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.
   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.
   You should have received a copy of the GNU General Public License
   along with this program.  If not, see <https://www.gnu.org/licenses/>.  */
/* As a special exception, you may create a larger work that contains
   part or all of the Bison parser skeleton and distribute that work
   under terms of your choice, so long as that work isn't itself a
   parser generator using the skeleton or a modified version thereof
   as a parser skeleton.  Alternatively, if you modify or redistribute
   the parser skeleton itself, you may (at your option) remove this
   special exception, which will cause the skeleton and the resulting
   Bison output files to be licensed under the GNU General Public
   License without this special exception.
   This special exception was added by the Free Software Foundation in
   version 2.2 of Bison.  */
/* C GLR parser skeleton written by Paul Hilfinger.  */
/* DO NOT RELY ON FEATURES THAT ARE NOT DOCUMENTED in the manual,
   especially those whose name start with YY_ or yy_.  They are
   private implementation details that can be changed or removed.  */
/* Identify Bison output, and Bison version.  */
#define YYBISON 30802
/* Bison version string.  */
#define YYBISON_VERSION "3.8.2"
/* Skeleton name.  */
#define YYSKELETON_NAME "glr.c"
/* Pure parsers.  */
#define YYPURE 1
/* Substitute the type names.  */
#define YYSTYPE MCA_STYPE
#define YYLTYPE MCA_LTYPE
/* Substitute the variable and function names.  */
#define yyparse mca_parse
#define yylex   mca_lex
#define yyerror mca_error
#define yydebug mca_debug
/* First part of user prologue.  */
    #include <stdio.h>
    #include <string.h>
    #include <stdlib.h>
    #include <assert.h>
    #include "astdef.h"
    #include "lex.h"
    #include "common.h"
    struct YYLTYPE;
    void mca_error(struct YYLTYPE *_loc, mc_value* mcast, const char *msg);
    extern mc_lex_token* g_last_token;
    #define YYMAXDEPTH 2000000
    /* Rule-span discipline. Every node built by a rule action records the
       span of the WHOLE reduction, not just its first subnode. The skeleton
       hands each action its rule location through `yylocp` (what the action
       spells `@$`; YYLLOC_DEFAULT fills it from the first to the last token
       of the rule, including tokens the action discards). Redirection by
       macro keeps the 383 call sites untouched; the spanned factory is
       defined in astdef.c. The macro lives only in this file: the lexer
       fills the location fields per token (lex.re, mca_lex). */
    #define mc_value_create_node(type, sub) \
        mc_value_create_node_spanned((type), (sub), \
            (unsigned int)(yylocp->first_column), \
            (unsigned int)(yylocp->last_column))
    /* U292 step 2: a literal's sign arrives as its own OP_PLUS/OP_MINUS
       token; the data string keeps the fat-token shape ("-3.3V") so the
       data-string readers see the same bytes, and the node span is the
       full rule span (sign token through literal). */
    static char* mc_sign_joined(const mc_value* v, char sign)
    {
        const char *body = v->data ? (const char*)v->data : "";
        size_t n = strlen(body);
        char *joined = (char*)malloc(n + 2);
        if (joined == NULL) return NULL;
        joined[0] = sign;
        memcpy(joined + 1, body, n + 1);
        return joined;
    }
    /* U363 close-out: rebuild a raw condition run from two node texts with
       an operator between them. Lossless modulo the original spacing (the
       same doctrine as the hyphen join below: the condition half is ONE
       MCAST_ID data node holding the raw text, judgment deferred to the
       engine batch). */
    static mc_value* mc_cond_join_op(mc_value* a, const char* op, mc_value* b)
    {
        const char *la = a->data ? (const char*)a->data : "";
        const char *lb = b->data ? (const char*)b->data : "";
        size_t ll = strlen(la);
        size_t lo = strlen(op);
        size_t lr = strlen(lb);
        char *joined = (char*)malloc(ll + lo + lr + 1);
        if (joined == NULL) return a;
        memcpy(joined, la, ll);
        memcpy(joined + ll, op, lo);
        memcpy(joined + ll + lo, lb, lr + 1);
        return mc_value_create_data(MCAST_ID, joined, a->pos, b->pos + b->len - a->pos);
    }
    /* U363 close-out: flatten a value subtree back to its raw text -- leaf
       data in source order (data, then children, then siblings). Used by
       the condition-call form `10mA@pm(1)`, whose argument list arrives as
       a structured mc_attr_values tree but whose condition half must stay
       one raw-text node. Loses the argument separators (commas live only
       in the token stream); the corpus call conditions are single-argument. */
    static void mc_cond_flatten(const mc_value* v, char** buf, size_t* len, size_t* cap)
    {
        if (v == NULL) return;
        const char *d = v->data ? (const char*)v->data : NULL;
        size_t dl = d ? strlen(d) : 0;
        if (dl > 0) {
            if (*len + dl + 1 > *cap) {
                *cap = (*len + dl + 1) * 2;
                *buf = (char*)realloc(*buf, *cap);
            }
            memcpy(*buf + *len, d, dl);
            *len += dl;
            (*buf)[*len] = '\0';
        }
        mc_cond_flatten(v->sub, buf, len, cap);
        mc_cond_flatten(v->next, buf, len, cap);
    }
    /* U363 close-out: the raw text of `word(args)` for the condition-call
       form. Caller frees nothing: ownership of the returned buffer moves
       into the MCAST_ID data node the grammar action builds. */
    static char* mc_cond_call_text(mc_value* word, mc_value* args)
    {
        size_t cap = 64;
        size_t len = 0;
        char *buf = (char*)malloc(cap);
        if (buf == NULL) return NULL;
        buf[0] = '\0';
        mc_cond_flatten(word, &buf, &len, &cap);
        if (len + 2 > cap) { cap = (len + 2) * 2; buf = (char*)realloc(buf, cap); }
        buf[len++] = '(';
        buf[len] = '\0';
        mc_cond_flatten(args, &buf, &len, &cap);
        if (len + 2 > cap) { cap = (len + 2) * 2; buf = (char*)realloc(buf, cap); }
        buf[len++] = ')';
        buf[len] = '\0';
        return buf;
    }
# ifndef YY_CAST
#  ifdef __cplusplus
#   define YY_CAST(Type, Val) static_cast<Type> (Val)
#   define YY_REINTERPRET_CAST(Type, Val) reinterpret_cast<Type> (Val)
#  else
#   define YY_CAST(Type, Val) ((Type) (Val))
#   define YY_REINTERPRET_CAST(Type, Val) ((Type) (Val))
#  endif
# endif
# ifndef YY_NULLPTR
#  if defined __cplusplus
#   if 201103L <= __cplusplus
#    define YY_NULLPTR nullptr
#   else
#    define YY_NULLPTR 0
#   endif
#  else
#   define YY_NULLPTR ((void*)0)
#  endif
# endif
#include "mca.tab.h"
/* Symbol kind.  */
enum yysymbol_kind_t
{
  YYSYMBOL_YYEMPTY = -2,
  YYSYMBOL_YYEOF = 0,                      /* "end of file"  */
  YYSYMBOL_YYerror = 1,                    /* error  */
  YYSYMBOL_YYUNDEF = 2,                    /* "invalid token"  */
  YYSYMBOL_MCTP_NUMBER_DEC = 3,            /* MCTP_NUMBER_DEC  */
  YYSYMBOL_MCTP_NUMBER_HEX = 4,            /* MCTP_NUMBER_HEX  */
  YYSYMBOL_MCTP_NUMBER_FLOAT = 5,          /* MCTP_NUMBER_FLOAT  */
  YYSYMBOL_MCTP_VERSION = 6,               /* MCTP_VERSION  */
  YYSYMBOL_MCTP_STRING = 7,                /* MCTP_STRING  */
  YYSYMBOL_MCTP_ID = 8,                    /* MCTP_ID  */
  YYSYMBOL_MCTP_IDA = 9,                   /* MCTP_IDA  */
  YYSYMBOL_MCOP_UNDERSCORE = 10,           /* MCOP_UNDERSCORE  */
  YYSYMBOL_MCK_PUB = 11,                   /* MCK_PUB  */
  YYSYMBOL_MCK_USE = 12,                   /* MCK_USE  */
  YYSYMBOL_MCPT_COLON = 13,                /* MCPT_COLON  */
  YYSYMBOL_MCK_AS = 14,                    /* MCK_AS  */
  YYSYMBOL_MCPT_DOT = 15,                  /* MCPT_DOT  */
  YYSYMBOL_MCPT_AT = 16,                   /* MCPT_AT  */
  YYSYMBOL_MCK_MC = 17,                    /* MCK_MC  */
  YYSYMBOL_MCK_COMPONENT = 18,             /* MCK_COMPONENT  */
  YYSYMBOL_MCK_MODULE = 19,                /* MCK_MODULE  */
  YYSYMBOL_MCK_INTERFACE = 20,             /* MCK_INTERFACE  */
  YYSYMBOL_MCK_ENUM = 21,                  /* MCK_ENUM  */
  YYSYMBOL_MCPT_LCURLY = 22,               /* MCPT_LCURLY  */
  YYSYMBOL_MCPT_RCURLY = 23,               /* MCPT_RCURLY  */
  YYSYMBOL_MCK_RECIPE = 24,                /* MCK_RECIPE  */
  YYSYMBOL_MCK_ABSTRACT = 25,              /* MCK_ABSTRACT  */
  YYSYMBOL_MCPT_SEMICOLON = 26,            /* MCPT_SEMICOLON  */
  YYSYMBOL_MCPT_COMMA = 27,                /* MCPT_COMMA  */
  YYSYMBOL_MCK_ROLE = 28,                  /* MCK_ROLE  */
  YYSYMBOL_MCK_CONDUIT = 29,               /* MCK_CONDUIT  */
  YYSYMBOL_MCK_DOMAIN = 30,                /* MCK_DOMAIN  */
  YYSYMBOL_MCK_RAIL = 31,                  /* MCK_RAIL  */
  YYSYMBOL_MCK_PARTITION = 32,             /* MCK_PARTITION  */
  YYSYMBOL_MCK_BOM = 33,                   /* MCK_BOM  */
  YYSYMBOL_MCOP_EQUAL = 34,                /* MCOP_EQUAL  */
  YYSYMBOL_MCK_PINS = 35,                  /* MCK_PINS  */
  YYSYMBOL_MCOP_PLUSEQUAL = 36,            /* MCOP_PLUSEQUAL  */
  YYSYMBOL_MCOP_EQUALEQUAL = 37,           /* MCOP_EQUALEQUAL  */
  YYSYMBOL_MCOP_NOTEQUAL = 38,             /* MCOP_NOTEQUAL  */
  YYSYMBOL_MCOP_LESSTHAN = 39,             /* MCOP_LESSTHAN  */
  YYSYMBOL_MCOP_GREATERTHAN = 40,          /* MCOP_GREATERTHAN  */
  YYSYMBOL_MCOP_LESSEQTHAN = 41,           /* MCOP_LESSEQTHAN  */
  YYSYMBOL_MCOP_GREATEREQTHAN = 42,        /* MCOP_GREATEREQTHAN  */
  YYSYMBOL_MCOP_DOUBLEARROW = 43,          /* MCOP_DOUBLEARROW  */
  YYSYMBOL_MCOP_LEFTARROW = 44,            /* MCOP_LEFTARROW  */
  YYSYMBOL_MCOP_RIGHTARROW = 45,           /* MCOP_RIGHTARROW  */
  YYSYMBOL_MCOP_PLUS = 46,                 /* MCOP_PLUS  */
  YYSYMBOL_MCOP_MINUS = 47,                /* MCOP_MINUS  */
  YYSYMBOL_MCOP_AND = 48,                  /* MCOP_AND  */
  YYSYMBOL_MCOP_OR = 49,                   /* MCOP_OR  */
  YYSYMBOL_MCOP_ANDAND = 50,               /* MCOP_ANDAND  */
  YYSYMBOL_MCOP_OROR = 51,                 /* MCOP_OROR  */
  YYSYMBOL_MCOP_MULTI = 52,                /* MCOP_MULTI  */
  YYSYMBOL_MCOP_DIVID = 53,                /* MCOP_DIVID  */
  YYSYMBOL_MCOP_CARET = 54,                /* MCOP_CARET  */
  YYSYMBOL_MCOP_APOST = 55,                /* MCOP_APOST  */
  YYSYMBOL_MCPT_LBRACKET = 56,             /* MCPT_LBRACKET  */
  YYSYMBOL_MCPT_RBRACKET = 57,             /* MCPT_RBRACKET  */
  YYSYMBOL_MCPT_LPAREN = 58,               /* MCPT_LPAREN  */
  YYSYMBOL_MCPT_RPAREN = 59,               /* MCPT_RPAREN  */
  YYSYMBOL_MCOP_TILDE = 60,                /* MCOP_TILDE  */
  YYSYMBOL_MCOP_PLUSMINUS = 61,            /* MCOP_PLUSMINUS  */
  YYSYMBOL_MCOP_TIMES = 62,                /* MCOP_TIMES  */
  YYSYMBOL_MCPT_DBCOLON = 63,              /* MCPT_DBCOLON  */
  YYSYMBOL_MCK_ELSE_IF = 64,               /* MCK_ELSE_IF  */
  YYSYMBOL_MCK_ELSE = 65,                  /* MCK_ELSE  */
  YYSYMBOL_MCK_IF = 66,                    /* MCK_IF  */
  YYSYMBOL_MC_ENDL = 67,                   /* MC_ENDL  */
  YYSYMBOL_MCK_RETURN = 68,                /* MCK_RETURN  */
  YYSYMBOL_MCK_ERROR = 69,                 /* MCK_ERROR  */
  YYSYMBOL_MCK_LET = 70,                   /* MCK_LET  */
  YYSYMBOL_MCK_REQUIRE = 71,               /* MCK_REQUIRE  */
  YYSYMBOL_MCK_IO = 72,                    /* MCK_IO  */
  YYSYMBOL_MCK_IN = 73,                    /* MCK_IN  */
  YYSYMBOL_MCK_OUT = 74,                   /* MCK_OUT  */
  YYSYMBOL_MCK_NC = 75,                    /* MCK_NC  */
  YYSYMBOL_MCK_PSRC = 76,                  /* MCK_PSRC  */
  YYSYMBOL_MCK_PSNK = 77,                  /* MCK_PSNK  */
  YYSYMBOL_MCK_PSBI = 78,                  /* MCK_PSBI  */
  YYSYMBOL_MCK_RF = 79,                    /* MCK_RF  */
  YYSYMBOL_MCONST_HIGH = 80,               /* MCONST_HIGH  */
  YYSYMBOL_MCONST_LOW = 81,                /* MCONST_LOW  */
  YYSYMBOL_MCONST_NC = 82,                 /* MCONST_NC  */
  YYSYMBOL_MCU_INT = 83,                   /* MCU_INT  */
  YYSYMBOL_MCU_HEX = 84,                   /* MCU_HEX  */
  YYSYMBOL_MCU_FLOAT = 85,                 /* MCU_FLOAT  */
  YYSYMBOL_MCU_STRING = 86,                /* MCU_STRING  */
  YYSYMBOL_MCK_FUNC = 87,                  /* MCK_FUNC  */
  YYSYMBOL_MCK_THIS = 88,                  /* MCK_THIS  */
  YYSYMBOL_MCU_VOLT = 89,                  /* MCU_VOLT  */
  YYSYMBOL_MCU_AMP = 90,                   /* MCU_AMP  */
  YYSYMBOL_MCU_CAP = 91,                   /* MCU_CAP  */
  YYSYMBOL_MCU_IND = 92,                   /* MCU_IND  */
  YYSYMBOL_MCU_TIME = 93,                  /* MCU_TIME  */
  YYSYMBOL_MCU_LEN = 94,                   /* MCU_LEN  */
  YYSYMBOL_MCU_WATT = 95,                  /* MCU_WATT  */
  YYSYMBOL_MCU_OHM = 96,                   /* MCU_OHM  */
  YYSYMBOL_MCU_TEMP = 97,                  /* MCU_TEMP  */
  YYSYMBOL_MCU_HZ = 98,                    /* MCU_HZ  */
  YYSYMBOL_MCU_DB = 99,                    /* MCU_DB  */
  YYSYMBOL_MCU_PPM = 100,                  /* MCU_PPM  */
  YYSYMBOL_MCU_PERCENT = 101,              /* MCU_PERCENT  */
  YYSYMBOL_MCU_BAUD = 102,                 /* MCU_BAUD  */
  YYSYMBOL_MCU_DATASIZE = 103,             /* MCU_DATASIZE  */
  YYSYMBOL_MCU_SPS = 104,                  /* MCU_SPS  */
  YYSYMBOL_MCU_SIEMENS = 105,              /* MCU_SIEMENS  */
  YYSYMBOL_MCU_RESPONSIVITY = 106,         /* MCU_RESPONSIVITY  */
  YYSYMBOL_MCU_ANGLE = 107,                /* MCU_ANGLE  */
  YYSYMBOL_MCU_ANGULAR_RATE = 108,         /* MCU_ANGULAR_RATE  */
  YYSYMBOL_MCU_ENERGY = 109,               /* MCU_ENERGY  */
  YYSYMBOL_MCU_EFIELD = 110,               /* MCU_EFIELD  */
  YYSYMBOL_MCU_HFIELD = 111,               /* MCU_HFIELD  */
  YYSYMBOL_MCU_FLUX = 112,                 /* MCU_FLUX  */
  YYSYMBOL_MCU_BFIELD = 113,               /* MCU_BFIELD  */
  YYSYMBOL_MCU_SLEW = 114,                 /* MCU_SLEW  */
  YYSYMBOL_MCU_NOISE = 115,                /* MCU_NOISE  */
  YYSYMBOL_MCU_CHARGE = 116,               /* MCU_CHARGE  */
  YYSYMBOL_MCUVAL_VOLT = 117,              /* MCUVAL_VOLT  */
  YYSYMBOL_MCUVAL_AMP = 118,               /* MCUVAL_AMP  */
  YYSYMBOL_MCUVAL_CAP = 119,               /* MCUVAL_CAP  */
  YYSYMBOL_MCUVAL_IND = 120,               /* MCUVAL_IND  */
  YYSYMBOL_MCUVAL_TIME = 121,              /* MCUVAL_TIME  */
  YYSYMBOL_MCUVAL_LEN = 122,               /* MCUVAL_LEN  */
  YYSYMBOL_MCUVAL_WATT = 123,              /* MCUVAL_WATT  */
  YYSYMBOL_MCUVAL_OHM = 124,               /* MCUVAL_OHM  */
  YYSYMBOL_MCUVAL_TEMP = 125,              /* MCUVAL_TEMP  */
  YYSYMBOL_MCUVAL_HZ = 126,                /* MCUVAL_HZ  */
  YYSYMBOL_MCUVAL_DB = 127,                /* MCUVAL_DB  */
  YYSYMBOL_MCUVAL_PPM = 128,               /* MCUVAL_PPM  */
  YYSYMBOL_MCUVAL_PERCENT = 129,           /* MCUVAL_PERCENT  */
  YYSYMBOL_MCUVAL_BAUD = 130,              /* MCUVAL_BAUD  */
  YYSYMBOL_MCUVAL_DATASIZE = 131,          /* MCUVAL_DATASIZE  */
  YYSYMBOL_MCUVAL_SPS = 132,               /* MCUVAL_SPS  */
  YYSYMBOL_MCUVAL_SIEMENS = 133,           /* MCUVAL_SIEMENS  */
  YYSYMBOL_MCUVAL_RESPONSIVITY = 134,      /* MCUVAL_RESPONSIVITY  */
  YYSYMBOL_MCUVAL_ANGLE = 135,             /* MCUVAL_ANGLE  */
  YYSYMBOL_MCUVAL_ANGULAR_RATE = 136,      /* MCUVAL_ANGULAR_RATE  */
  YYSYMBOL_MCUVAL_ENERGY = 137,            /* MCUVAL_ENERGY  */
  YYSYMBOL_MCUVAL_EFIELD = 138,            /* MCUVAL_EFIELD  */
  YYSYMBOL_MCUVAL_HFIELD = 139,            /* MCUVAL_HFIELD  */
  YYSYMBOL_MCUVAL_FLUX = 140,              /* MCUVAL_FLUX  */
  YYSYMBOL_MCUVAL_BFIELD = 141,            /* MCUVAL_BFIELD  */
  YYSYMBOL_MCUVAL_SLEW = 142,              /* MCUVAL_SLEW  */
  YYSYMBOL_MCUVAL_NOISE = 143,             /* MCUVAL_NOISE  */
  YYSYMBOL_MCUVAL_CHARGE = 144,            /* MCUVAL_CHARGE  */
  YYSYMBOL_MCUVAL_COMPOSITE = 145,         /* MCUVAL_COMPOSITE  */
  YYSYMBOL_MC_WS = 146,                    /* MC_WS  */
  YYSYMBOL_MC_SINGLE_COMMENT = 147,        /* MC_SINGLE_COMMENT  */
  YYSYMBOL_MC_MULTI_COMMENT = 148,         /* MC_MULTI_COMMENT  */
  YYSYMBOL_UVALUE_PAIR_PREC = 149,         /* UVALUE_PAIR_PREC  */
  YYSYMBOL_UVALUE_WORD_PREC = 150,         /* UVALUE_WORD_PREC  */
  YYSYMBOL_IDA_BASE_PREC = 151,            /* IDA_BASE_PREC  */
  YYSYMBOL_YYACCEPT = 152,                 /* $accept  */
  YYSYMBOL_start = 153,                    /* start  */
  YYSYMBOL_mc_tops = 154,                  /* mc_tops  */
  YYSYMBOL_mc_top = 155,                   /* mc_top  */
  YYSYMBOL_mc_use = 156,                   /* mc_use  */
  YYSYMBOL_mc_uri = 157,                   /* mc_uri  */
  YYSYMBOL_mc_prefix = 158,                /* mc_prefix  */
  YYSYMBOL_mc_uri_trunk = 159,             /* mc_uri_trunk  */
  YYSYMBOL_mc_levels = 160,                /* mc_levels  */
  YYSYMBOL_mc_class_name = 161,            /* mc_class_name  */
  YYSYMBOL_mc_component = 162,             /* mc_component  */
  YYSYMBOL_mc_module = 163,                /* mc_module  */
  YYSYMBOL_mc_interface = 164,             /* mc_interface  */
  YYSYMBOL_mc_enum = 165,                  /* mc_enum  */
  YYSYMBOL_mc_bom = 166,                   /* mc_bom  */
  YYSYMBOL_mc_recipe = 167,                /* mc_recipe  */
  YYSYMBOL_mc_component_derivs = 168,      /* mc_component_derivs  */
  YYSYMBOL_mc_component_variant = 169,     /* mc_component_variant  */
  YYSYMBOL_mc_component_adopts = 170,      /* mc_component_adopts  */
  YYSYMBOL_mc_body = 171,                  /* mc_body  */
  YYSYMBOL_mc_clauses = 172,               /* mc_clauses  */
  YYSYMBOL_mc_clause = 173,                /* mc_clause  */
  YYSYMBOL_mc_attribute = 174,             /* mc_attribute  */
  YYSYMBOL_mc_attr_values = 175,           /* mc_attr_values  */
  YYSYMBOL_mc_attr_value = 176,            /* mc_attr_value  */
  YYSYMBOL_mc_attr_line_t = 177,           /* mc_attr_line_t  */
  YYSYMBOL_mc_attr_lines = 178,            /* mc_attr_lines  */
  YYSYMBOL_mc_attribute_pin = 179,         /* mc_attribute_pin  */
  YYSYMBOL_mc_pins_lines = 180,            /* mc_pins_lines  */
  YYSYMBOL_mc_pins_line = 181,             /* mc_pins_line  */
  YYSYMBOL_mc_pin_idn = 182,               /* mc_pin_idn  */
  YYSYMBOL_mc_pins_names = 183,            /* mc_pins_names  */
  YYSYMBOL_mc_pins_name = 184,             /* mc_pins_name  */
  YYSYMBOL_mc_nc_list = 185,               /* mc_nc_list  */
  YYSYMBOL_mc_net = 186,                   /* mc_net  */
  YYSYMBOL_mc_opds = 187,                  /* mc_opds  */
  YYSYMBOL_mc_net_port_elem = 188,         /* mc_net_port_elem  */
  YYSYMBOL_mc_net_port_elems = 189,        /* mc_net_port_elems  */
  YYSYMBOL_mc_mn_opd = 190,                /* mc_mn_opd  */
  YYSYMBOL_mc_mn_opds = 191,               /* mc_mn_opds  */
  YYSYMBOL_mc_opd = 192,                   /* mc_opd  */
  YYSYMBOL_mc_phrases = 193,               /* mc_phrases  */
  YYSYMBOL_mc_list_items = 194,            /* mc_list_items  */
  YYSYMBOL_mc_list_sep = 195,              /* mc_list_sep  */
  YYSYMBOL_mc_list_item = 196,             /* mc_list_item  */
  YYSYMBOL_mc_phrase = 197,                /* mc_phrase  */
  YYSYMBOL_mc_role = 198,                  /* mc_role  */
  YYSYMBOL_mc_conduit = 199,               /* mc_conduit  */
  YYSYMBOL_mc_domain = 200,                /* mc_domain  */
  YYSYMBOL_mc_rail = 201,                  /* mc_rail  */
  YYSYMBOL_mc_partition = 202,             /* mc_partition  */
  YYSYMBOL_mc_error_stmt = 203,            /* mc_error_stmt  */
  YYSYMBOL_mc_error_msg = 204,             /* mc_error_msg  */
  YYSYMBOL_mc_tattrs = 205,                /* mc_tattrs  */
  YYSYMBOL_mc_tattrs_opt = 206,            /* mc_tattrs_opt  */
  YYSYMBOL_mc_tattr = 207,                 /* mc_tattr  */
  YYSYMBOL_mc_tattr_args = 208,            /* mc_tattr_args  */
  YYSYMBOL_mc_tattr_arg = 209,             /* mc_tattr_arg  */
  YYSYMBOL_mc_tkey = 210,                  /* mc_tkey  */
  YYSYMBOL_mc_resword = 211,               /* mc_resword  */
  YYSYMBOL_mc_function = 212,              /* mc_function  */
  YYSYMBOL_mc_paramds = 213,               /* mc_paramds  */
  YYSYMBOL_mc_pards = 214,                 /* mc_pards  */
  YYSYMBOL_mc_pard = 215,                  /* mc_pard  */
  YYSYMBOL_mc_declare_a = 216,             /* mc_declare_a  */
  YYSYMBOL_mc_declare_a1 = 217,            /* mc_declare_a1  */
  YYSYMBOL_mc_insts = 218,                 /* mc_insts  */
  YYSYMBOL_mc_inst = 219,                  /* mc_inst  */
  YYSYMBOL_mc_iface_name = 220,            /* mc_iface_name  */
  YYSYMBOL_mc_declare_b = 221,             /* mc_declare_b  */
  YYSYMBOL_mc_params = 222,                /* mc_params  */
  YYSYMBOL_mc_body_params = 223,           /* mc_body_params  */
  YYSYMBOL_mc_param = 224,                 /* mc_param  */
  YYSYMBOL_mc_conds = 225,                 /* mc_conds  */
  YYSYMBOL_mc_conds_elifs = 226,           /* mc_conds_elifs  */
  YYSYMBOL_mc_cond_block = 227,            /* mc_cond_block  */
  YYSYMBOL_mc_expr = 228,                  /* mc_expr  */
  YYSYMBOL_mc_judge = 229,                 /* mc_judge  */
  YYSYMBOL_mc_id = 230,                    /* mc_id  */
  YYSYMBOL_mc_ida = 231,                   /* mc_ida  */
  YYSYMBOL_mc_ida_tail = 232,              /* mc_ida_tail  */
  YYSYMBOL_mc_ida_bracket = 233,           /* mc_ida_bracket  */
  YYSYMBOL_mc_ida_items = 234,             /* mc_ida_items  */
  YYSYMBOL_mc_ida_item = 235,              /* mc_ida_item  */
  YYSYMBOL_mc_reorder_items = 236,         /* mc_reorder_items  */
  YYSYMBOL_mc_reorder_item = 237,          /* mc_reorder_item  */
  YYSYMBOL_mc_ida_run = 238,               /* mc_ida_run  */
  YYSYMBOL_mc_ida_run_atom = 239,          /* mc_ida_run_atom  */
  YYSYMBOL_mc_idss = 240,                  /* mc_idss  */
  YYSYMBOL_mc_ids = 241,                   /* mc_ids  */
  YYSYMBOL_mc_idseg = 242,                 /* mc_idseg  */
  YYSYMBOL_mc_idm = 243,                   /* mc_idm  */
  YYSYMBOL_mc_idans = 244,                 /* mc_idans  */
  YYSYMBOL_mc_idan = 245,                  /* mc_idan  */
  YYSYMBOL_mc_int = 246,                   /* mc_int  */
  YYSYMBOL_mc_hex = 247,                   /* mc_hex  */
  YYSYMBOL_mc_float = 248,                 /* mc_float  */
  YYSYMBOL_mc_number = 249,                /* mc_number  */
  YYSYMBOL_mc_string = 250,                /* mc_string  */
  YYSYMBOL_mc_const = 251,                 /* mc_const  */
  YYSYMBOL_mc_nc = 252,                    /* mc_nc  */
  YYSYMBOL_mc_underscore = 253,            /* mc_underscore  */
  YYSYMBOL_mc_literal = 254,               /* mc_literal  */
  YYSYMBOL_mc_cond_word = 255,             /* mc_cond_word  */
  YYSYMBOL_mc_cond_rel = 256,              /* mc_cond_rel  */
  YYSYMBOL_mc_cond_rel_head = 257,         /* mc_cond_rel_head  */
  YYSYMBOL_mc_iotype = 258,                /* mc_iotype  */
  YYSYMBOL_mc_iotype_rf = 259,             /* mc_iotype_rf  */
  YYSYMBOL_mc_pin_iotype = 260,            /* mc_pin_iotype  */
  YYSYMBOL_mc_unit_value = 261,            /* mc_unit_value  */
  YYSYMBOL_mc_unit_type = 262,             /* mc_unit_type  */
  YYSYMBOL_mc_endls = 263                  /* mc_endls  */
};
typedef enum yysymbol_kind_t yysymbol_kind_t;
/* Default (constant) value used for initialization for null
   right-hand sides.  Unlike the standard yacc.c template, here we set
   the default value of $$ to a zeroed-out value.  Since the default
   value is undefined, this behavior is technically correct.  */
static YYSTYPE yyval_default;
static YYLTYPE yyloc_default
# if defined MCA_LTYPE_IS_TRIVIAL && MCA_LTYPE_IS_TRIVIAL
  = { 1, 1, 1, 1 }
# endif
;
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef short
# undef short
#endif
/* On compilers that do not define __PTRDIFF_MAX__ etc., make sure
   <limits.h> and (if available) <stdint.h> are included
   so that the code can choose integer types of a good width.  */
#ifndef __PTRDIFF_MAX__
# include <limits.h> /* INFRINGES ON USER NAME SPACE */
# if defined __STDC_VERSION__ && 199901 <= __STDC_VERSION__
#  include <stdint.h> /* INFRINGES ON USER NAME SPACE */
#  define YY_STDINT_H
# endif
#endif
/* Narrow types that promote to a signed type and that can represent a
   signed or unsigned integer of at least N bits.  In tables they can
   save space and decrease cache pressure.  Promoting to a signed type
   helps avoid bugs in integer arithmetic.  */
#ifdef __INT_LEAST8_MAX__
typedef __INT_LEAST8_TYPE__ yytype_int8;
#elif defined YY_STDINT_H
typedef int_least8_t yytype_int8;
#else
typedef signed char yytype_int8;
#endif
#ifdef __INT_LEAST16_MAX__
typedef __INT_LEAST16_TYPE__ yytype_int16;
#elif defined YY_STDINT_H
typedef int_least16_t yytype_int16;
#else
typedef short yytype_int16;
#endif
/* Work around bug in HP-UX 11.23, which defines these macros
   incorrectly for preprocessor constants.  This workaround can likely
   be removed in 2023, as HPE has promised support for HP-UX 11.23
   (aka HP-UX 11i v2) only through the end of 2022; see Table 2 of
   <https://h20195.www2.hpe.com/V2/getpdf.aspx/4AA4-7673ENW.pdf>.  */
#ifdef __hpux
# undef UINT_LEAST8_MAX
# undef UINT_LEAST16_MAX
# define UINT_LEAST8_MAX 255
# define UINT_LEAST16_MAX 65535
#endif
#if defined __UINT_LEAST8_MAX__ && __UINT_LEAST8_MAX__ <= __INT_MAX__
typedef __UINT_LEAST8_TYPE__ yytype_uint8;
#elif (!defined __UINT_LEAST8_MAX__ && defined YY_STDINT_H \
       && UINT_LEAST8_MAX <= INT_MAX)
typedef uint_least8_t yytype_uint8;
#elif !defined __UINT_LEAST8_MAX__ && UCHAR_MAX <= INT_MAX
typedef unsigned char yytype_uint8;
#else
typedef short yytype_uint8;
#endif
#if defined __UINT_LEAST16_MAX__ && __UINT_LEAST16_MAX__ <= __INT_MAX__
typedef __UINT_LEAST16_TYPE__ yytype_uint16;
#elif (!defined __UINT_LEAST16_MAX__ && defined YY_STDINT_H \
       && UINT_LEAST16_MAX <= INT_MAX)
typedef uint_least16_t yytype_uint16;
#elif !defined __UINT_LEAST16_MAX__ && USHRT_MAX <= INT_MAX
typedef unsigned short yytype_uint16;
#else
typedef int yytype_uint16;
#endif
#ifndef YYPTRDIFF_T
# if defined __PTRDIFF_TYPE__ && defined __PTRDIFF_MAX__
#  define YYPTRDIFF_T __PTRDIFF_TYPE__
#  define YYPTRDIFF_MAXIMUM __PTRDIFF_MAX__
# elif defined PTRDIFF_MAX
#  ifndef ptrdiff_t
#   include <stddef.h> /* INFRINGES ON USER NAME SPACE */
#  endif
#  define YYPTRDIFF_T ptrdiff_t
#  define YYPTRDIFF_MAXIMUM PTRDIFF_MAX
# else
#  define YYPTRDIFF_T long
#  define YYPTRDIFF_MAXIMUM LONG_MAX
# endif
#endif
#ifndef YYSIZE_T
# ifdef __SIZE_TYPE__
#  define YYSIZE_T __SIZE_TYPE__
# elif defined size_t
#  define YYSIZE_T size_t
# elif defined __STDC_VERSION__ && 199901 <= __STDC_VERSION__
#  include <stddef.h> /* INFRINGES ON USER NAME SPACE */
#  define YYSIZE_T size_t
# else
#  define YYSIZE_T unsigned
# endif
#endif
#define YYSIZE_MAXIMUM                                  \
  YY_CAST (YYPTRDIFF_T,                                 \
           (YYPTRDIFF_MAXIMUM < YY_CAST (YYSIZE_T, -1)  \
            ? YYPTRDIFF_MAXIMUM                         \
            : YY_CAST (YYSIZE_T, -1)))
#define YYSIZEOF(X) YY_CAST (YYPTRDIFF_T, sizeof (X))
#ifndef YY_
# if defined YYENABLE_NLS && YYENABLE_NLS
#  if ENABLE_NLS
#   include <libintl.h> /* INFRINGES ON USER NAME SPACE */
#   define YY_(Msgid) dgettext ("bison-runtime", Msgid)
#  endif
# endif
# ifndef YY_
#  define YY_(Msgid) Msgid
# endif
#endif
#ifndef YYFREE
# define YYFREE free
#endif
#ifndef YYMALLOC
# define YYMALLOC malloc
#endif
#ifndef YYREALLOC
# define YYREALLOC realloc
#endif
#ifdef __cplusplus
  typedef bool yybool;
# define yytrue true
# define yyfalse false
#else
  /* When we move to stdbool, get rid of the various casts to yybool.  */
  typedef signed char yybool;
# define yytrue 1
# define yyfalse 0
#endif
#ifndef YYSETJMP
# include <setjmp.h>
# define YYJMP_BUF jmp_buf
# define YYSETJMP(Env) setjmp (Env)
/* Pacify Clang and ICC.  */
# define YYLONGJMP(Env, Val)                    \
 do {                                           \
   longjmp (Env, Val);                          \
   YY_ASSERT (0);                               \
 } while (yyfalse)
#endif
#ifndef YY_ATTRIBUTE_PURE
# if defined __GNUC__ && 2 < __GNUC__ + (96 <= __GNUC_MINOR__)
#  define YY_ATTRIBUTE_PURE __attribute__ ((__pure__))
# else
#  define YY_ATTRIBUTE_PURE
# endif
#endif
#ifndef YY_ATTRIBUTE_UNUSED
# if defined __GNUC__ && 2 < __GNUC__ + (7 <= __GNUC_MINOR__)
#  define YY_ATTRIBUTE_UNUSED __attribute__ ((__unused__))
# else
#  define YY_ATTRIBUTE_UNUSED
# endif
#endif
/* The _Noreturn keyword of C11.  */
#ifndef _Noreturn
# if (defined __cplusplus \
      && ((201103 <= __cplusplus && !(__GNUC__ == 4 && __GNUC_MINOR__ == 7)) \
          || (defined _MSC_VER && 1900 <= _MSC_VER)))
#  define _Noreturn [[noreturn]]
# elif ((!defined __cplusplus || defined __clang__) \
        && (201112 <= (defined __STDC_VERSION__ ? __STDC_VERSION__ : 0) \
            || (!defined __STRICT_ANSI__ \
                && (4 < __GNUC__ + (7 <= __GNUC_MINOR__) \
                    || (defined __apple_build_version__ \
                        ? 6000000 <= __apple_build_version__ \
                        : 3 < __clang_major__ + (5 <= __clang_minor__))))))
   /* _Noreturn works as-is.  */
# elif (2 < __GNUC__ + (8 <= __GNUC_MINOR__) || defined __clang__ \
        || 0x5110 <= __SUNPRO_C)
#  define _Noreturn __attribute__ ((__noreturn__))
# elif 1200 <= (defined _MSC_VER ? _MSC_VER : 0)
#  define _Noreturn __declspec (noreturn)
# else
#  define _Noreturn
# endif
#endif
/* Suppress unused-variable warnings by "using" E.  */
#if ! defined lint || defined __GNUC__
# define YY_USE(E) ((void) (E))
#else
# define YY_USE(E) /* empty */
#endif
/* Suppress an incorrect diagnostic about yylval being uninitialized.  */
#if defined __GNUC__ && ! defined __ICC && 406 <= __GNUC__ * 100 + __GNUC_MINOR__
# if __GNUC__ * 100 + __GNUC_MINOR__ < 407
#  define YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN                           \
    _Pragma ("GCC diagnostic push")                                     \
    _Pragma ("GCC diagnostic ignored \"-Wuninitialized\"")
# else
#  define YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN                           \
    _Pragma ("GCC diagnostic push")                                     \
    _Pragma ("GCC diagnostic ignored \"-Wuninitialized\"")              \
    _Pragma ("GCC diagnostic ignored \"-Wmaybe-uninitialized\"")
# endif
# define YY_IGNORE_MAYBE_UNINITIALIZED_END      \
    _Pragma ("GCC diagnostic pop")
#else
# define YY_INITIAL_VALUE(Value) Value
#endif
#ifndef YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN
# define YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN
# define YY_IGNORE_MAYBE_UNINITIALIZED_END
#endif
#ifndef YY_INITIAL_VALUE
# define YY_INITIAL_VALUE(Value) /* Nothing. */
#endif
#if defined __cplusplus && defined __GNUC__ && ! defined __ICC && 6 <= __GNUC__
# define YY_IGNORE_USELESS_CAST_BEGIN                          \
    _Pragma ("GCC diagnostic push")                            \
    _Pragma ("GCC diagnostic ignored \"-Wuseless-cast\"")
# define YY_IGNORE_USELESS_CAST_END            \
    _Pragma ("GCC diagnostic pop")
#endif
#ifndef YY_IGNORE_USELESS_CAST_BEGIN
# define YY_IGNORE_USELESS_CAST_BEGIN
# define YY_IGNORE_USELESS_CAST_END
#endif
#define YY_ASSERT(E) ((void) (0 && (E)))
/* YYFINAL -- State number of the termination state.  */
#define YYFINAL  46
/* YYLAST -- Last index in YYTABLE.  */
#define YYLAST   5916
/* YYNTOKENS -- Number of terminals.  */
#define YYNTOKENS  152
/* YYNNTS -- Number of nonterminals.  */
#define YYNNTS  112
/* YYNRULES -- Number of rules.  */
#define YYNRULES  633
/* YYNSTATES -- Number of states.  */
#define YYNSTATES  1019
/* YYMAXRHS -- Maximum number of symbols on right-hand side of rule.  */
#define YYMAXRHS 9
/* YYMAXLEFT -- Maximum number of symbols to the left of a handle
   accessed by $0, $-1, etc., in any rule.  */
#define YYMAXLEFT 0
/* YYMAXUTOK -- Last valid token kind.  */
#define YYMAXUTOK   406
/* YYTRANSLATE(TOKEN-NUM) -- Symbol number corresponding to TOKEN-NUM
   as returned by yylex, with out-of-bounds checking.  */
#define YYTRANSLATE(YYX)                                \
  (0 <= (YYX) && (YYX) <= YYMAXUTOK                     \
   ? YY_CAST (yysymbol_kind_t, yytranslate[YYX])        \
   : YYSYMBOL_YYUNDEF)
/* YYTRANSLATE[TOKEN-NUM] -- Symbol number corresponding to TOKEN-NUM
   as returned by yylex.  */
static const yytype_uint8 yytranslate[] =
{
       0,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     2,     2,     2,     2,
       2,     2,     2,     2,     2,     2,     1,     2,     3,     4,
       5,     6,     7,     8,     9,    10,    11,    12,    13,    14,
      15,    16,    17,    18,    19,    20,    21,    22,    23,    24,
      25,    26,    27,    28,    29,    30,    31,    32,    33,    34,
      35,    36,    37,    38,    39,    40,    41,    42,    43,    44,
      45,    46,    47,    48,    49,    50,    51,    52,    53,    54,
      55,    56,    57,    58,    59,    60,    61,    62,    63,    64,
      65,    66,    67,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    77,    78,    79,    80,    81,    82,    83,    84,
      85,    86,    87,    88,    89,    90,    91,    92,    93,    94,
      95,    96,    97,    98,    99,   100,   101,   102,   103,   104,
     105,   106,   107,   108,   109,   110,   111,   112,   113,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,   146,   147,   148,   149,   150,   151
};
#if MCA_DEBUG
/* YYRLINE[YYN] -- source line where rule number YYN was defined.  */
static const yytype_int16 yyrline[] =
{
       0,   447,   447,   448,   449,   450,   451,   453,   454,   456,
     457,   458,   459,   460,   461,   462,   463,   471,   476,   490,
     495,   514,   522,   532,   538,   546,   550,   560,   561,   562,
     564,   568,   573,   578,   583,   589,   600,   616,   617,   618,
     619,   620,   621,   625,   626,   635,   646,   660,   670,   680,
     692,   698,   706,   709,   711,   713,   715,   718,   731,   732,
     733,   735,   736,   737,   738,   739,   740,   741,   742,   743,
     744,   745,   750,   761,   766,   769,   788,   805,   820,   831,
     836,   843,   847,   851,   855,   860,   892,   901,   902,   903,
     904,   908,   918,   928,   941,   942,   943,   945,   952,   960,
     968,   978,   990,   999,  1010,  1021,  1031,  1036,  1041,  1046,
    1051,  1058,  1068,  1074,  1079,  1084,  1091,  1092,  1097,  1102,
    1107,  1112,  1121,  1134,  1135,  1149,  1150,  1151,  1152,  1168,
    1169,  1170,  1175,  1176,  1179,  1184,  1189,  1195,  1201,  1209,
    1221,  1227,  1233,  1241,  1250,  1255,  1262,  1267,  1272,  1277,
    1278,  1279,  1293,  1298,  1304,  1308,  1313,  1314,  1315,  1325,
    1343,  1352,  1353,  1355,  1356,  1357,  1358,  1372,  1377,  1390,
    1391,  1392,  1393,  1394,  1395,  1396,  1397,  1398,  1400,  1401,
    1402,  1403,  1404,  1405,  1406,  1407,  1408,  1410,  1411,  1412,
    1413,  1414,  1415,  1416,  1417,  1418,  1427,  1428,  1429,  1431,
    1432,  1433,  1434,  1435,  1436,  1437,  1438,  1439,  1441,  1442,
    1443,  1444,  1445,  1446,  1447,  1448,  1449,  1451,  1452,  1453,
    1454,  1455,  1456,  1457,  1458,  1459,  1461,  1462,  1463,  1464,
    1466,  1467,  1468,  1469,  1471,  1476,  1481,  1486,  1505,  1525,
    1536,  1544,  1564,  1596,  1601,  1607,  1612,  1619,  1627,  1637,
    1647,  1656,  1665,  1673,  1681,  1689,  1699,  1708,  1719,  1730,
    1740,  1749,  1758,  1767,  1801,  1808,  1817,  1824,  1835,  1840,
    1848,  1862,  1868,  1874,  1897,  1905,  1923,  1933,  1937,  1941,
    1945,  1949,  1953,  1966,  1971,  1978,  1981,  1986,  1991,  2008,
    2017,  2022,  2028,  2039,  2043,  2047,  2059,  2059,  2059,  2059,
    2059,  2059,  2059,  2059,  2061,  2073,  2085,  2086,  2087,  2089,
    2090,  2094,  2098,  2106,  2112,  2119,  2126,  2132,  2138,  2144,
    2150,  2156,  2166,  2175,  2191,  2203,  2211,  2220,  2228,  2236,
    2245,  2250,  2257,  2262,  2272,  2273,  2274,  2281,  2289,  2299,
    2300,  2301,  2313,  2314,  2315,  2316,  2319,  2323,  2327,  2331,
    2339,  2346,  2362,  2375,  2384,  2396,  2408,  2413,  2422,  2427,
    2439,  2444,  2451,  2456,  2461,  2468,  2469,  2470,  2472,  2473,
    2474,  2475,  2476,  2477,  2478,  2479,  2480,  2481,  2482,  2483,
    2484,  2495,  2503,  2512,  2513,  2520,  2521,  2522,  2523,  2528,
    2542,  2543,  2544,  2545,  2550,  2551,  2552,  2553,  2557,  2563,
    2563,  2563,  2564,  2564,  2564,  2564,  2564,  2564,  2564,  2564,
    2565,  2565,  2565,  2565,  2565,  2565,  2565,  2565,  2566,  2566,
    2566,  2566,  2566,  2566,  2567,  2567,  2567,  2567,  2567,  2567,
    2567,  2567,  2567,  2567,  2568,  2568,  2569,  2569,  2569,  2570,
    2570,  2570,  2570,  2571,  2571,  2571,  2571,  2571,  2571,  2571,
    2571,  2571,  2571,  2572,  2572,  2572,  2572,  2572,  2572,  2572,
    2572,  2573,  2573,  2573,  2573,  2573,  2573,  2573,  2574,  2574,
    2574,  2575,  2575,  2575,  2575,  2575,  2575,  2575,  2575,  2576,
    2576,  2576,  2576,  2576,  2576,  2576,  2577,  2577,  2577,  2577,
    2577,  2577,  2578,  2578,  2578,  2578,  2578,  2578,  2578,  2578,
    2581,  2582,  2583,  2586,  2587,  2588,  2589,  2590,  2591,  2592,
    2593,  2594,  2595,  2596,  2597,  2598,  2600,  2601,  2602,  2603,
    2604,  2605,  2607,  2609,  2610,  2611,  2612,  2617,  2618,  2619,
    2620,  2624,  2638,  2652,  2663,  2677,  2681,  2685,  2701,  2707,
    2713,  2718,  2736,  2740,  2744,  2748,  2752,  2756,  2760,  2764,
    2774,  2778,  2782,  2786,  2791,  2795,  2801,  2802,  2803,  2804,
    2805,  2806,  2807,  2812,  2813,  2813,  2820,  2821,  2822,  2823,
    2824,  2825,  2826,  2827,  2828,  2829,  2830,  2831,  2832,  2833,
    2834,  2835,  2836,  2837,  2838,  2839,  2840,  2841,  2842,  2843,
    2844,  2845,  2846,  2847,  2848,  2852,  2853,  2854,  2855,  2856,
    2857,  2858,  2859,  2860,  2861,  2862,  2863,  2864,  2865,  2866,
    2867,  2868,  2869,  2870,  2871,  2872,  2873,  2874,  2875,  2876,
    2877,  2878,  2879,  2880,  2881,  2882,  2883,  2884,  2889,  2894,
    2900,  2901,  2902,  2903
};
#endif
#define YYPACT_NINF (-871)
#define YYTABLE_NINF (-517)
/* YYPACT[STATE-NUM] -- Index in YYTABLE of the portion describing
   STATE-NUM.  */
static const yytype_int16 yypact[] =
{
     963,  -871,    73,    86,   529,   529,   529,   529,   529,    36,
    -871,   529,  -871,    69,    48,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,   641,   103,  -871,  -871,    82,  -871,    70,
     118,   146,   718,  -871,   128,   136,  -871,  -871,   197,   333,
     136,   136,   213,   244,   529,   244,  -871,   843,  -871,  -871,
      48,  -871,   271,   234,  -871,   529,   340,   529,  -871,   118,
     667,  -871,   118,   118,   118,   118,  4198,    57,  -871,   753,
     385,   397,   529,   646,  -871,   244,   244,   529,  3187,  -871,
     136,  -871,  -871,   932,   529,  -871,   391,  -871,   529,  -871,
    -871,   395,   399,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  4341,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,   179,
    -871,   434,  -871,   128,  -871,  -871,   421,   663,  5485,   166,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,   666,   459,
      71,  -871,  -871,  -871,   171,  -871,  5485,   529,   244,   401,
    -871,  -871,  -871,   734,   120,  -871,   517,  -871,  -871,   668,
    -871,  -871,  -871,  -871,   114,   529,   529,   529,  5485,   636,
     565,   988,   988,  4913,  5485,   988,  5628,  5485,   409,   118,
    5628,  -871,  -871,   118,   737,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,   125,  -871,  -871,  -871,  -871,   835,
    1912,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,   545,
     548,  -871,   492,   500,  -871,  -871,  -871,  -871,  -871,  -871,
     516,  5485,   567,   385,   391,   529,  -871,  -871,  -871,   280,
    4341,  -871,  4341,  -871,   529,   646,   590,   761,   835,   301,
    2268,   282,   516,   559,   613,   646,   615,  4627,   529,   469,
    -871,   397,  1860,  4627,  2279,  -871,   593,  2268,   454,  -871,
    -871,   529,  -871,  -871,  -871,  -871,   646,   397,  -871,  5485,
    -871,   449,  -871,  -871,   675,   647,   647,  2268,  -871,   244,
     646,   114,   614,   633,  -871,  -871,  -871,  -871,  4913,  2007,
     377,   236,  -871,  1684,   649,   269,  1578,  2121,  -871,  -871,
    5628,  1428,  2654,   664,  3042,  1330,   672,  2605,   690,   765,
     136,   114,  -871,  3477,  5485,   118,    99,   230,  5485,  5485,
    5485,  5485,  5485,  5485,  -871,  -871,  5485,   397,  5485,   646,
      72,  5771,   250,  5485,  5485,  5485,  5485,  5485,  5485,  -871,
    -871,  5485,   397,   647,  -871,  -871,   646,   646,  4627,   397,
    5056,  5056,  4627,   700,   709,  5485,  5485,  5485,  5485,  5485,
    5485,   397,  -871,    87,   835,  1912,   358,  2452,   244,  -871,
     684,  -871,  -871,  -871,  -871,   397,   114,  5485,   682,  -871,
     397,  -871,   397,  -871,   777,  -871,   835,  2268,   138,  -871,
     240,  -871,   516,   693,   738,  -871,   766,  -871,  -871,   144,
    2279,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,   586,  4627,   391,  -871,  -871,  -871,   380,
    -871,   114,   114,  3187,  -871,   244,  -871,   773,   188,   715,
     699,    45,  3767,  3767,   835,  -871,  2268,   516,   682,  -871,
    -871,  4913,    52,  4627,  5485,  5485,  -871,   993,  2121,   597,
     739,  5485,  5485,  5485,  5485,  5485,  5485,  5485,  5485,  5485,
    3332,  5628,  5628,  -871,  -871,   758,  3477,   754,   762,   778,
    5056,   244,   229,  -871,  1039,   438,   168,   774,  4341,   293,
     166,  5485,  -871,   521,   734,   734,   964,  1147,   964,  1147,
     964,  1147,   215,   964,  1147,   215,   735,   451,  -871,   735,
     451,  -871,  2033,  1173,   389,  -871,  1039,   438,   168,   776,
    -871,  -871,  -871,  -871,   782,  -871,  4055,  4913,   431,   835,
    2164,   528,   166,  -871,   658,   734,   734,   964,  1147,   964,
    1147,   964,  1147,   215,   964,  1147,   215,   735,   451,  -871,
     735,   451,  -871,  2033,  1173,   389,  -871,  -871,   757,   790,
     205,   515,   791,  5199,   833,  -871,   835,  2268,   516,   833,
     246,   529,   529,  1039,   438,   168,   964,  1147,   215,   964,
    1147,   215,   735,   451,  -871,   735,   451,  -871,  2033,  1173,
     389,  -871,   153,  -871,  -871,  2640,  2640,  2640,  2640,  -871,
     860,  -871,   805,  -871,  -871,  -871,   835,  2268,   516,   529,
    -871,  -871,   114,  4627,   803,  5056,  5056,  4627,   397,  -871,
     529,   695,  5485,  2279,  2279,   286,  -871,  -871,   810,   141,
    -871,   816,   114,   840,  -871,  -871,   111,  -871,   851,  2268,
     852,   857,  -871,  -871,  5485,   132,  -871,   290,   298,   335,
    -871,  5485,  1798,  -871,   630,  -871,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,   834,  5628,  3622,   794,  -871,  2605,
    -871,  2605,   833,  -871,   114,  4627,   812,  -871,   881,   114,
     484,  -871,  4627,  -871,  -871,  4627,  4484,   537,   623,   671,
     745,   822,   381,   114,  -871,   505,  4627,  -871,  -871,  -871,
    -871,  -871,  5056,   700,   709,  -871,   781,  -871,  -871,   156,
     566,  5056,   529,  -871,  -871,  -871,   459,  -871,   862,  -871,
     862,   862,   862,  2640,  2640,  2640,  2640,  2640,  1195,  5056,
    2640,   861,   829,  -871,   529,   647,   647,   334,  -871,  -871,
    -871,  -871,  2268,   516,  -871,  -871,  -871,  -871,  -871,  3767,
     848,  5056,  -871,  3912,  5342,   886,  -871,   647,  -871,  -871,
     474,  3042,  -871,  5628,  3622,  -871,  -871,  -871,  -871,   850,
     344,   899,  4341,  4341,   888,   166,   244,   361,   384,  -871,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,   402,   448,  -871,
     864,   108,   901,   682,   889,   244,   455,  -871,   833,  -871,
     124,   397,  5056,  -871,   700,   709,   118,  -871,  -871,  -871,
    -871,  -871,  -871,  -871,   462,  -871,  4627,   892,   869,   647,
     647,  4627,  -871,  2845,   257,  -871,   833,  -871,  4770,   314,
    -871,   835,  2268,    79,  -871,  5342,   647,  -871,  -871,  3042,
    -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,  -871,   803,
    -871,  -871,   118,  -871,  -871,  5056,  -871,  -871,  -871,   803,
     906,  -871,  -871,   589,   515,   594,  -871,  -871,   526,  5056,
    4627,  -871,  -871,  -871,   491,  -871,  5056,  5342,   606,   405,
    -871,  -871,  -871,  -871,   895,  -871,  -871,   646,   397,   647,
    -871,  -871,   570,   849,  -871,   833,  -871,  5056,  5056,   648,
     894,   896,  -871,  -871,   833,   833,  5056,   840,   833
};
/* YYDEFACT[STATE-NUM] -- Default reduction number in state STATE-NUM.
   Performed when YYTABLE does not specify something else to do.  Zero
   means the default is an error.  */
static const yytype_int16 yydefact[] =
{
       0,    16,     0,     0,     0,     0,     0,     0,     0,     0,
     631,     0,   630,     0,     2,     8,     9,    10,    11,    12,
      13,    14,    15,     0,     0,    21,   382,     0,    27,    19,
       0,    23,     0,    42,     0,   308,   383,   504,    44,   503,
     308,   308,     0,     0,     0,     0,     1,     0,   633,   632,
       4,    22,    17,     0,    28,     0,    25,     0,    36,     0,
       0,    33,     0,     0,     0,     0,     0,   384,   385,     0,
      54,     0,     0,     0,   505,     0,     0,     0,     0,    51,
     308,    50,     7,     0,     0,    29,    20,   502,     0,    24,
      37,    32,    31,    40,    39,    41,    38,   400,   401,   399,
     402,   403,   404,   405,   406,   407,   408,   409,   410,   411,
     412,   413,   414,   415,   416,   417,   418,     0,   420,   421,
     419,   422,   423,   434,   435,   424,   425,   426,   427,   428,
     429,   430,   431,   436,   437,   438,   439,   440,   441,   442,
     432,   433,   443,   444,   445,   446,   447,   448,   449,   450,
     451,   452,   453,   454,   455,   456,   457,   458,   459,   460,
     461,   462,   463,   464,   465,   466,   467,   468,   469,   470,
     471,   472,   473,   474,   475,   476,   477,   478,   479,   480,
     481,   482,   483,   484,   485,   486,   487,   488,   489,   490,
     491,   492,   493,   494,   495,   496,   497,   498,   499,     0,
     390,   392,   398,     0,   386,   526,   311,   140,     0,     0,
     307,   558,   556,   557,   562,   559,   560,   561,   136,     0,
       0,   310,   319,   320,   313,   144,     0,     0,     0,    56,
     516,    43,   506,   512,     0,   510,   513,    47,    48,     0,
      74,   517,   518,   522,     0,     0,     0,     0,     0,     0,
     140,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   523,   524,     0,   136,   566,   567,   568,   569,   570,
     571,   572,   573,   574,   575,   576,   577,   578,   579,   580,
     581,   582,   583,   584,   585,   586,   587,   588,   589,   590,
     591,   592,   593,   594,     0,    59,    61,    62,    63,     0,
     285,    64,    67,    68,    69,    70,    71,    65,   120,   161,
     162,    66,   383,   134,   519,   520,   521,   527,   528,   529,
       0,     0,   530,    54,    18,   501,    26,    35,    34,     0,
       0,   388,     0,   387,     0,     0,   141,   140,   150,     0,
     149,   134,   151,     0,   134,     0,   137,   341,     0,     0,
     306,     0,     0,   341,     0,   327,   332,   317,   134,    53,
      45,     0,    52,   507,   515,   508,     0,     0,    49,     0,
     133,     0,   129,   130,     0,   285,   285,   273,   275,     0,
       0,     0,     0,     0,   541,   539,   540,   538,     0,   150,
       0,     0,   153,   149,   383,   151,     0,     0,   537,   536,
       0,   366,   367,     0,     0,   381,   122,     0,     0,    73,
     308,     0,    57,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   165,   163,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   166,
     164,     0,     0,   286,   121,   283,     0,     0,   341,     0,
       0,     0,   341,   324,   327,     0,     0,     0,     0,     0,
       0,     0,   128,   285,   125,   285,   162,     0,     0,   500,
       0,   391,   393,   312,   142,     0,     0,     0,   316,   145,
       0,   138,     0,   348,   140,   525,   349,   350,     0,   340,
     134,   347,   346,     0,   334,   309,   135,   314,   315,     0,
       0,   595,   596,   597,   598,   599,   600,   601,   602,   603,
     609,   610,   604,   605,   606,   607,   608,   611,   612,   613,
     614,   615,   616,   617,   618,   619,   620,   621,   622,   623,
     624,   625,   626,   321,   341,    55,   509,   511,   514,     0,
     241,     0,     0,     0,   271,     0,   274,   383,     0,   504,
       0,   130,     0,     0,   157,   160,   156,   158,     0,   154,
     245,     0,   155,   341,     0,     0,   246,   366,   367,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   364,   362,   356,     0,   278,     0,   277,
       0,     0,     0,    58,   217,   223,   218,     0,     0,     0,
       0,     0,   264,     0,   252,   254,   230,   232,   226,   228,
     169,   175,   170,   178,   184,   179,   187,   193,   188,   199,
     205,   200,   208,   214,   209,   196,   221,   225,   222,   383,
     235,   234,   294,   295,   287,   293,     0,     0,     0,   129,
       0,   519,     0,   266,     0,   253,   255,   231,   233,   227,
     229,   173,   177,   174,   182,   186,   183,   191,   195,   192,
     203,   207,   204,   212,   216,   213,   198,   284,     0,     0,
       0,   135,     0,     0,    75,    80,    82,    83,    81,    77,
       0,     0,     0,   219,   224,   220,   171,   176,   172,   180,
     185,   181,   189,   194,   190,   201,   206,   202,   210,   215,
     211,   197,     0,   118,   119,     0,     0,     0,     0,   542,
     532,   535,   531,    46,   389,   143,   148,   146,   147,     0,
     135,   139,     0,     0,     0,     0,     0,   345,     0,   328,
       0,     0,     0,     0,     0,     0,   131,   132,     0,     0,
     272,     0,     0,   508,   105,   563,     0,    95,     0,   108,
     504,   519,   564,   565,     0,     0,   152,     0,     0,     0,
     380,     0,   366,   368,   365,   371,   372,   373,   374,   375,
     376,   377,   378,   369,   370,     0,     0,   358,   363,     0,
     276,     0,    72,   305,     0,   341,     0,   394,   396,     0,
       0,   124,   341,   260,   262,   341,     0,   400,   382,   140,
     136,     0,     0,     0,   236,     0,   341,   261,   263,   243,
     244,   247,     0,   325,   328,    85,   140,    89,    90,     0,
     134,     0,     0,   330,   331,   127,   125,   126,   550,   554,
     551,   552,   553,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   339,     0,   353,   351,     0,   344,   335,
     329,   629,   323,   322,   628,   627,   333,   242,   270,     0,
       0,     0,    91,     0,     0,     0,    92,   247,   268,   269,
       0,     0,   357,     0,     0,   279,   280,   281,   282,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   298,
     296,   297,   299,   300,   301,   302,   303,     0,     0,   291,
       0,   383,     0,   131,     0,     0,     0,   256,    78,    84,
       0,     0,     0,    79,   326,   329,     0,   546,   547,   548,
     549,   545,   543,   544,     0,   533,   345,   508,     0,   354,
     352,     0,   336,     0,     0,   238,    76,    94,     0,    97,
     110,   114,   115,   519,   111,     0,   159,   379,   360,     0,
     359,   237,   248,   168,   395,   397,   239,   123,   265,   250,
     249,   288,     0,   289,   304,     0,   167,   240,   267,   251,
       0,    87,    88,     0,   135,    75,   555,   534,     0,     0,
     345,   342,   343,    93,     0,   117,     0,     0,   101,    99,
     361,   257,   258,   290,     0,   292,   259,     0,     0,    86,
     338,   355,     0,     0,   112,    98,   109,     0,     0,   102,
       0,     0,   337,   116,   103,   100,     0,     0,   104
};
/* YYPGOTO[NTERM-NUM].  */
static const yytype_int16 yypgoto[] =
{
    -871,  -871,   912,    42,  -871,   913,  -871,   909,  -871,    85,
    -871,  -871,  -871,  -871,  -871,  -871,   617,  -871,  -871,   -35,
     398,   -60,  -641,  -413,  -690,    35,  -871,  -871,  -532,    84,
     195,     9,   -28,  -871,  -871,   319,   261,  -871,   425,  -332,
     823,  -169,  -871,  -871,  -344,  1694,  -871,  -871,  -871,  -871,
    -871,  -871,  -871,  -794,  -311,  -427,  -871,     5,  -871,  -871,
    -871,   -21,  -871,   619,  -871,   -40,  -626,  -306,  -671,   -45,
    -336,  -870,  -700,  -871,  -871,  -733,   730,  -247,    -2,  1336,
    -871,   -20,   853,   639,   337,    89,  -317,  -871,   -51,   362,
    -871,   -14,  -370,   610,  1112,  -871,  -871,   553,  -871,  -871,
    -818,  -871,  2083,  -871,  -871,  -406,   -57,  -871,  -871,  -221,
    -463,     0
};
/* YYDEFGOTO[NTERM-NUM].  */
static const yytype_int16 yydefgoto[] =
{
       0,    13,    14,    15,    16,    29,    30,    31,    32,    35,
      17,    18,    19,    20,    21,    22,   228,   229,   362,   583,
     294,   584,   296,   674,   675,   818,   819,   297,   746,   747,
     748,   939,   940,   984,   298,   790,   462,   463,   370,   371,
     299,   390,   391,   561,   392,   677,   301,   302,   303,   304,
     305,   306,   588,   443,   444,   445,   898,   899,   634,   900,
     307,    70,   220,   221,   308,   309,   453,   355,   493,   310,
     488,   847,   489,   311,   777,   585,   403,   404,   312,    37,
      67,    68,   199,   200,   786,   787,   201,   202,    86,   341,
      39,   336,   234,   235,   314,   315,   316,   317,   318,   319,
     491,   225,   320,   710,   711,   828,   321,   753,   754,   322,
     533,   586
};
/* YYTABLE[YYPACT[STATE-NUM]] -- What to do in state STATE-NUM.  If
   positive, shift that token.  If negative, reduce the rule whose
   number is the opposite.  If YYTABLE_NINF, syntax error.  */
static const yytype_int16 yytable[] =
{
      23,    33,    36,    36,    36,    36,    36,   454,    79,    36,
      81,   550,   226,   409,    47,   472,   667,   499,   295,    75,
      76,   755,    33,   843,   223,    74,   239,   848,    33,   222,
     385,   387,   817,   324,   399,   845,   846,   731,   679,   339,
     237,   238,    36,   872,   555,   813,   944,   204,   841,   548,
      83,   929,   930,    36,    44,    36,   978,    90,   367,   323,
      93,    94,    95,    96,   544,   545,   203,    36,  -513,    46,
      36,    36,  -130,   946,    10,    36,   668,   669,    48,   592,
      26,    34,    36,    55,   599,    24,    36,    25,   406,    82,
      40,    41,    42,    43,    26,  -113,    45,    53,   349,   638,
     632,    27,   230,   430,    51,  -113,  -113,    26,    34,   205,
    1002,    26,   670,    66,   702,    12,   680,   230,    27,    49,
     985,   598,    26,    34,   205,    82,    26,   944,  -113,    80,
     350,   913,    26,    34,   207,    54,  -113,    10,   948,    28,
     633,   950,   965,   365,   548,   988,  -113,   366,   412,   207,
      48,    10,   703,   569,   704,   369,    28,   209,    10,   970,
      57,    26,    34,   205,   858,   723,   448,    10,   862,   944,
     369,   723,   209,   928,    26,    34,   205,   782,    12,    26,
      34,   999,    10,   333,    66,  1013,   351,   218,   207,   866,
     729,    49,    12,   360,    69,  1009,   914,   724,   735,    12,
     539,   207,   218,   730,   346,   352,   330,    36,    12,   601,
     738,   209,    71,   909,   378,   541,   990,   756,   363,   364,
     458,   459,    36,    12,   209,    36,   848,   757,   455,   353,
     461,   981,   723,   982,   354,    77,   331,   742,    26,    34,
     205,   218,    36,    36,    36,    36,   712,    36,    26,    34,
     346,   394,    78,   725,   218,   351,   541,   408,    26,    34,
     205,   410,    10,   559,   811,   207,    78,   458,   459,   971,
     854,   855,    78,   723,   726,   995,   466,   461,   784,   600,
     848,   788,   455,    10,    84,   207,   601,    85,   209,  1001,
      26,    34,   226,   560,   413,  -158,  -151,   351,   353,   642,
     830,   831,   832,    12,   223,   822,   601,   330,   209,   222,
     535,    36,   359,   723,   983,   456,   457,   723,   218,   788,
     541,   458,   459,    36,    12,   477,  -151,   934,   477,   460,
     430,   461,    36,    36,   773,   774,  -158,   470,   218,    36,
     353,   986,   789,    36,   546,   856,    36,    36,    72,   867,
      36,   536,   842,   593,    88,    73,    36,   868,   478,    36,
      10,   931,   477,   987,    36,   814,    38,    38,    38,    38,
      38,   723,   605,    38,  -126,   823,   824,   604,   547,    36,
     447,  -126,   602,   897,  -126,  -126,   394,   646,   723,   591,
     548,   562,   645,   932,   869,   758,   759,   643,   227,   908,
     230,    12,   455,   952,   477,    36,    38,   477,   477,    36,
     860,   723,   327,   597,    36,    36,   328,    87,   325,    89,
     959,   430,  -126,  -126,   850,  -126,   924,   629,    36,   821,
      36,   224,  1008,   713,   558,   456,   457,   736,   903,    87,
     313,   458,   459,   960,    36,    36,    87,   332,   936,   880,
     326,   461,   879,   429,   987,   334,   887,   884,   541,   888,
     431,   961,    26,    34,   361,   709,   429,   407,   802,   351,
     906,   904,   540,   431,    36,   962,   541,    26,    34,   205,
     803,  -318,   723,   295,   829,   829,   829,   829,    36,   821,
     437,   438,   439,   440,    36,   752,   752,   206,   542,   975,
     442,   477,   667,   667,   207,   439,   440,   963,    26,    34,
     740,   885,   353,  -318,   969,   449,   915,   347,  1003,   667,
     295,   977,   348,    26,    34,   208,   778,   209,   871,   455,
     367,   947,   885,   886,   450,   364,   451,    26,    34,    36,
      36,   211,   212,   213,   214,   215,   216,   217,  1004,   812,
     448,  -130,    10,   931,   905,  -130,   783,   218,   452,   394,
    -400,   667,   456,   457,  -400,   788,   955,   446,   458,   459,
     447,   344,   667,  1005,    26,    34,   460,  -130,   461,   792,
     380,   911,   667,   467,   348,  1000,  -516,   381,   358,    38,
     793,   794,   870,    12,  1014,  1015,    10,   931,    36,   382,
     912,   383,   451,  1018,   998,   475,   344,   374,   375,   376,
     430,   379,   917,   918,   919,   920,   921,   923,   479,   925,
     732,   821,   430,   912,   353,   451,   949,  1010,   480,  1012,
     482,   807,   808,  1007,    36,   394,  -399,    12,   733,   734,
      36,    -6,     1,   455,    26,    34,  -382,   581,   582,   230,
    -382,   534,     2,     3,    26,    34,   760,   827,    78,     4,
       5,     6,     7,   430,   430,     8,     9,    48,   729,    36,
     552,   394,    91,    92,    11,  1016,   456,   457,   335,    36,
      36,   345,   458,   459,  -418,    73,   335,   469,    73,   553,
     460,   368,   461,    73,  -140,   325,   473,   543,  -140,   477,
      36,   571,   572,   573,   574,   575,   576,   563,    49,   490,
     494,   224,   577,   578,   497,   490,   806,    36,   -30,    58,
      36,   348,   743,    87,   590,   579,   366,   681,    36,   536,
      72,   -30,   -30,    59,    60,    61,   682,    73,  -512,   413,
      36,   714,  -504,   344,   -30,   719,   863,   733,   734,    72,
     415,   727,   345,   728,   851,   863,    73,   416,  -433,   411,
     345,    26,    34,   205,    62,    63,   313,    73,  -136,   344,
      64,    65,  -136,   344,    26,   313,   335,    36,   344,    36,
     809,   206,    36,   476,   366,   -30,   346,    36,   207,   424,
     425,   630,   335,   347,   901,   761,   335,   907,   348,   722,
     779,    36,   752,   381,   384,   386,   752,   741,   398,   208,
     490,   209,   210,   810,   490,   581,   582,   366,    36,   910,
      36,   780,   775,   776,   781,   211,   212,   213,   214,   215,
     216,   217,   785,   857,   795,   881,   922,   541,   344,   882,
     796,   218,    36,    -3,     1,   902,   815,   933,   414,   882,
     415,   958,   927,   907,     2,     3,   366,   416,   873,   874,
     821,     4,     5,     6,     7,   840,   844,     8,     9,    48,
     968,   935,   859,   951,   861,   541,    11,   541,   417,   418,
     419,   420,   421,    36,   581,   864,  -107,   422,   423,   424,
     425,  -106,   219,   347,   883,   426,   490,   427,   348,   833,
     834,   835,   836,   344,   344,   313,   837,   838,    36,   916,
      49,   956,   967,   536,   976,   541,   541,  1017,   839,   926,
     945,   366,   953,   964,   966,   490,   979,   980,   997,   965,
     812,   485,    -5,     1,   863,    50,   394,    52,   991,    56,
     468,   739,   313,     2,     3,   972,   992,   937,   313,   865,
       4,     5,     6,     7,   989,   996,     8,     9,    48,  1006,
     994,   805,   344,   825,     1,    11,   737,   993,   495,   471,
     329,   954,    36,   801,     2,     3,   537,   414,   933,   415,
       0,     4,     5,     6,     7,     0,   416,     8,     9,    10,
       0,   230,   241,   242,     0,    36,    11,     0,   344,    49,
       0,     0,   933,     0,   344,     0,   414,   417,   415,     0,
       0,     0,     0,     0,     0,   416,   422,   423,   424,   425,
     564,     0,   347,     0,     0,     0,   427,   348,     0,     0,
      12,   338,   343,     0,     0,   820,   417,   418,   419,   420,
     421,     0,     0,     0,     0,   422,   423,   424,   425,     0,
       0,   347,   479,   426,   415,   427,   348,     0,     0,     0,
       0,   416,     0,     0,   344,     0,   570,   372,     0,     0,
       0,     0,     0,     0,     0,     0,   389,   396,     0,   401,
     338,   494,     0,   401,   344,   490,     0,     0,     0,   490,
       0,   422,   423,   424,   425,     0,     0,   347,     0,     0,
       0,   427,   348,     0,   344,   265,   266,   267,   268,   269,
     270,   271,   272,   273,   274,   275,   276,   277,   278,   279,
     280,   281,   282,   283,   284,   285,   286,   287,   288,   289,
     290,   291,   292,   293,     0,     0,     0,     0,   313,     0,
       0,   344,     0,   344,   464,     0,   344,   490,     0,     0,
       0,   344,     0,     0,   490,     0,     0,   490,     0,     0,
     428,     0,   429,     0,     0,   344,     0,     0,   490,   431,
     486,     0,   219,     0,     0,     0,   486,     0,     0,     0,
       0,     0,     0,   231,     0,   236,   428,     0,   429,     0,
     432,     0,   338,     0,     0,   431,     0,     0,     0,   437,
     438,   439,   440,    26,   372,     0,   494,     0,     0,   442,
       0,   554,     0,     0,     0,     0,   432,   433,   434,   435,
     436,     0,     0,   567,     0,   437,   438,   439,   440,     0,
     587,     0,     0,   313,   372,   442,   313,   594,     0,   372,
     603,   606,   608,   610,   613,   616,   619,   344,     0,   622,
       0,   626,     0,     0,   639,   644,   647,   649,   651,   654,
     657,   660,     0,     0,   663,     0,     0,     0,     0,     0,
       0,   486,   973,   676,   676,   486,     0,     0,   683,   686,
     689,   692,   695,   698,     0,     0,     0,     0,   490,     0,
       0,     0,     0,   490,     0,   490,     0,     0,     0,   372,
     716,   763,   765,   766,   767,   768,   769,   770,   771,   772,
       0,   313,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,   286,   287,   288,   289,   290,   291,   292,
     293,     0,   490,   455,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   373,   486,     0,     0,
       0,     0,     0,     0,   372,   372,     0,  -365,  -365,  -365,
    -365,  -365,  -365,     0,     0,     0,   456,   457,  -365,  -365,
       0,     0,   458,   459,   554,     0,   486,   338,   338,     0,
     460,  -365,   461,     0,   762,   762,   762,   762,   762,   762,
     762,   762,   762,   639,   401,   401,     0,     0,   232,   233,
       0,     0,     0,   676,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   791,   338,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   414,     0,   415,     0,     0,     0,   474,     0,     0,
     416,     0,     0,     0,     0,     0,     0,   481,     0,   372,
     389,     0,     0,   496,     0,   791,     0,     0,     0,     0,
       0,   417,   418,   419,   420,   421,     0,     0,   236,   538,
     422,   423,   424,   425,     0,     0,   347,     0,   426,     0,
     427,   348,   474,   551,     0,     0,   389,     0,     0,     0,
       0,   570,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   551,     0,   826,     0,     0,   373,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   625,
       0,   631,     0,   641,     0,   372,   486,     0,   676,   676,
     486,     0,     0,     0,   666,     0,     0,     0,   236,   236,
     356,   671,     0,     0,     0,   372,     0,     0,     0,     0,
       0,     0,     0,   701,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   338,     0,     0,   715,   551,     0,
       0,   414,   720,   415,   721,     0,     0,     0,   401,     0,
     416,     0,   875,     0,   877,   564,     0,   372,   486,     0,
       0,     0,   372,     0,     0,   486,     0,     0,   486,   676,
       0,   417,   418,   419,   420,   421,   372,     0,     0,   486,
     422,   423,   424,   425,     0,   676,   347,   479,   426,     0,
     427,   348,     0,     0,   676,     0,     0,     0,     0,   356,
       0,     0,     0,   373,   373,     0,     0,     0,     0,     0,
       0,     0,   676,     0,   751,   751,     0,     0,     0,     0,
       0,   232,     0,     0,     0,     0,     0,   356,     0,     0,
       0,   232,     0,     0,   676,     0,     0,   941,     0,     0,
       0,     0,   641,     0,   356,     0,   401,   428,     0,   429,
       0,     0,   233,     0,     0,     0,   431,     0,   957,     0,
    -156,  -149,     0,     0,     0,     0,   232,   549,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   432,   433,   434,
     435,   436,     0,     0,     0,   676,   437,   438,   439,   440,
       0,  -149,     0,     0,   441,     0,   442,   549,   373,   486,
       0,  -156,     0,     0,   486,     0,   486,     0,     0,     0,
       0,   389,     0,     0,     0,     0,   635,     0,   941,     0,
       0,     0,   300,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   233,   233,     0,     0,     0,     0,   676,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   676,   486,     0,     0,     0,     0,     0,   676,
     941,   414,   549,   415,     0,     0,     0,     0,     0,     0,
     416,     0,     0,     0,     0,     0,   356,     0,     0,     0,
     676,   676,   356,     0,   551,     0,     0,     0,     0,   676,
     849,   417,   418,   419,   420,   421,     0,     0,     0,     0,
     422,   423,   424,   425,   373,     0,   347,     0,   426,     0,
     427,   348,     0,   230,   241,   242,   751,   243,    26,    34,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   750,   750,
       0,     0,     0,     0,     0,     0,   373,     0,     0,     0,
       0,   373,   340,     0,     0,     0,   251,   252,     0,     0,
       0,     0,     0,     0,     0,   373,     0,     0,     0,     0,
     357,   255,     0,     0,     0,   428,     0,   429,   430,     0,
       0,     0,     0,     0,   431,     0,     0,     0,     0,     0,
     261,   262,   377,     0,     0,     0,     0,   393,   397,     0,
     402,   340,     0,     0,   402,   432,   433,   434,   435,   436,
       0,     0,     0,     0,   437,   438,   439,   440,     0,     0,
       0,   751,   441,     0,   442,   751,   943,   265,   266,   267,
     268,   269,   270,   271,   272,   273,   274,   275,   276,   277,
     278,   279,   280,   281,   282,   283,   284,   285,   286,   287,
     288,   289,   290,   291,   292,   293,     0,   356,     0,     0,
       0,     0,     0,     0,     0,   465,     0,   356,   356,     0,
     414,     0,   415,   974,     0,     0,     0,     0,     0,   416,
       0,     0,     0,  -157,  -150,     0,     0,     0,     0,     0,
       0,   487,     0,     0,     0,     0,   414,   487,   415,     0,
     417,   418,   419,   420,   421,   416,     0,   943,   549,   422,
     423,   424,   425,   340,  -150,   347,   356,   426,     0,   427,
     348,     0,     0,     0,  -157,     0,   417,   418,   419,   420,
     421,     0,   556,     0,     0,   422,   423,   424,   425,     0,
     750,   347,     0,     0,   568,   427,   348,     0,   300,   943,
       0,     0,     0,     0,     0,     0,     0,   300,   595,   236,
    1011,     0,   607,   609,   611,   614,   617,   620,     0,     0,
     623,     0,   627,     0,     0,   640,     0,   648,   650,   652,
     655,   658,   661,     0,   428,   664,   429,     0,     0,     0,
       0,     0,   487,   431,     0,     0,   487,     0,   565,   684,
     687,   690,   693,   696,   699,     0,   356,     0,   356,     0,
       0,     0,     0,     0,   432,   433,   434,   435,   436,     0,
       0,   717,     0,   437,   438,   439,   440,   428,     0,   429,
     566,   441,     0,   442,     0,     0,   431,   804,     0,     0,
       0,     0,     0,     0,     0,   750,     0,     0,     0,   750,
       0,     0,     0,     0,     0,     0,     0,   432,   433,   434,
     435,   436,     0,     0,     0,     0,   437,   438,   439,   440,
       0,     0,     0,     0,   441,     0,   442,     0,   487,     0,
       0,     0,     0,     0,     0,     0,     0,   300,     0,     0,
       0,     0,     0,     0,     0,     0,   749,   749,     0,     0,
       0,     0,     0,     0,     0,   556,     0,   487,   340,   340,
       0,     0,     0,     0,     0,   402,   402,   402,   402,   402,
     402,   402,   402,   402,   300,   402,   402,     0,     0,     0,
     300,   428,     0,   429,     0,     0,     0,     0,     0,     0,
     431,   342,     0,     0,     0,   340,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     356,   432,   433,   434,   435,   436,     0,     0,     0,     0,
     437,   438,   439,   440,     0,     0,     0,     0,   441,     0,
     442,   393,     0,   233,     0,     0,   395,   500,     0,   405,
     342,     0,     0,   405,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   501,   502,   503,   504,     0,   393,   505,   506,
     507,   508,   509,   510,   511,   512,   513,   514,   515,   516,
     517,   518,   519,   520,   521,   522,   523,   524,   525,   526,
     527,   528,   529,   530,   531,   532,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   487,     0,     0,
       0,   487,     0,     0,     0,     0,   852,     0,     0,     0,
     492,     0,     0,     0,     0,   498,   492,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   749,     0,
       0,     0,   342,     0,     0,   340,     0,     0,     0,     0,
      26,     0,     0,     0,     0,     0,     0,     0,     0,   402,
     300,   557,     0,     0,     0,     0,     0,     0,     0,   487,
       0,     0,     0,   405,     0,     0,   487,     0,     0,   487,
     589,   705,   706,   707,   708,     0,     0,   596,     0,     0,
     487,     0,     0,   612,   615,   618,   621,     0,     0,   624,
       0,   628,     0,     0,     0,     0,     0,     0,   653,   656,
     659,   662,     0,     0,   665,     0,     0,     0,     0,     0,
       0,   492,     0,   678,   678,   492,     0,     0,   685,   688,
     691,   694,   697,   700,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   749,     0,     0,     0,   749,   942,     0,
     718,     0,     0,     0,     0,   300,     0,   402,   300,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   230,   241,
     242,     0,   243,    26,    34,   205,     0,   492,     0,     0,
     487,     0,     0,     0,     0,   487,     0,   487,     0,     0,
       0,     0,   393,     0,     0,     0,     0,     0,     0,   942,
     207,     0,     0,   300,   557,     0,   492,   342,   342,     0,
       0,   251,   252,     0,   764,   764,   764,   764,   764,   764,
     764,   764,   764,   209,   405,   405,   255,   428,     0,   429,
       0,     0,     0,   678,   487,     0,   431,     0,     0,     0,
       0,   942,     0,     0,   342,   261,   262,     0,     0,     0,
       0,     0,     0,   218,     0,     0,     0,   432,   433,   434,
     435,   436,     0,     0,     0,     0,   437,   438,   439,   440,
       0,     0,     0,     0,   441,     0,   442,     0,     0,     0,
     395,     0,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,   286,   287,   288,   289,   290,   291,   292,
     293,     0,     0,     0,     0,     0,   395,   265,   266,   267,
     268,   269,   270,   271,   272,   273,   274,   275,   276,   277,
     278,   279,   280,   281,   282,   283,   284,   285,   286,   287,
     288,   289,   290,   291,   292,   293,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   492,     0,   678,   678,
     492,     0,     0,     0,     0,   853,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   342,     0,     0,     0,   230,   241,
     242,     0,   243,    26,    34,   205,     0,     0,   405,     0,
       0,     0,   876,     0,   878,     0,     0,   244,   492,     0,
       0,    48,     0,   483,     0,   492,     0,     0,   492,   678,
     484,     0,     0,     0,     0,     0,     0,     0,     0,   492,
       0,   251,   252,     0,     0,   678,     0,     0,     0,     0,
       0,   253,     0,   254,   678,     0,   255,     0,     0,     0,
       0,     0,    49,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   678,     0,     0,   261,   262,   485,     0,     0,
       0,     0,     0,   264,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   678,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   405,     0,     0,     0,
       0,     0,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,   286,   287,   288,   289,   290,   291,   292,
     293,     0,     0,     0,     0,   678,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   492,
       0,     0,     0,     0,   492,     0,   492,     0,     0,     0,
       0,   395,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   240,     0,   230,   241,   242,   678,   243,
      26,    34,   205,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   678,   492,   580,     0,     0,     0,    10,   678,
     245,   246,   247,   248,   249,     0,     0,   250,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   251,   252,
     678,   678,   581,   582,     0,     0,     0,     0,   253,   678,
     254,     0,     0,   255,     0,     0,     0,     0,   256,    12,
     257,   258,   259,   260,   211,   212,   213,   214,   215,   216,
     217,     0,   261,   262,     0,     0,     0,     0,     0,   263,
     264,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,   240,     0,
     230,   241,   242,     0,   243,    26,    34,   205,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   244,
     -60,     0,     0,   -60,     0,   245,   246,   247,   248,   249,
       0,     0,   250,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   251,   252,     0,     0,     0,     0,     0,
       0,     0,     0,   253,     0,   254,     0,     0,   255,     0,
       0,     0,     0,   256,   -60,   257,   258,   259,   260,   211,
     212,   213,   214,   215,   216,   217,     0,   261,   262,     0,
       0,     0,     0,     0,   263,   264,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   265,   266,   267,   268,   269,   270,
     271,   272,   273,   274,   275,   276,   277,   278,   279,   280,
     281,   282,   283,   284,   285,   286,   287,   288,   289,   290,
     291,   292,   293,   240,     0,   230,   241,   242,     0,   243,
      26,    34,   205,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   244,   -60,     0,     0,   -60,     0,
     245,   246,   247,   248,   249,     0,     0,   250,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   251,   252,
       0,     0,     0,     0,     0,     0,     0,     0,   637,     0,
     254,     0,     0,   255,     0,     0,     0,     0,   256,   -60,
     257,   258,   259,   260,   211,   212,   213,   214,   215,   216,
     217,     0,   261,   262,     0,     0,     0,     0,     0,   263,
     264,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,   240,     0,
     230,   241,   242,     0,   243,    26,    34,   205,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   244,
       0,     0,     0,    48,     0,   245,   246,   247,   248,   249,
       0,     0,   250,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   251,   252,     0,     0,     0,     0,     0,
       0,     0,     0,   253,     0,   254,     0,     0,   255,     0,
       0,     0,     0,   256,    49,   257,   258,   259,   260,   211,
     212,   213,   214,   215,   216,   217,     0,   261,   262,     0,
       0,     0,     0,     0,   263,   264,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   265,   266,   267,   268,   269,   270,
     271,   272,   273,   274,   275,   276,   277,   278,   279,   280,
     281,   282,   283,   284,   285,   286,   287,   288,   289,   290,
     291,   292,   293,   240,     0,   230,   241,   242,     0,   243,
      26,    34,   205,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   580,     0,     0,     0,    10,     0,
     245,   246,   247,   248,   249,     0,     0,   250,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   251,   252,
       0,     0,     0,     0,     0,     0,     0,     0,   253,     0,
     254,     0,     0,   255,     0,     0,     0,     0,   256,    12,
     257,   258,   259,   260,   211,   212,   213,   214,   215,   216,
     217,     0,   261,   262,     0,     0,     0,     0,     0,   263,
     264,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,   744,     0,
     230,   241,   242,     0,   243,    26,    34,   205,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   244,
       0,     0,     0,   -96,     0,     0,     0,     0,     0,     0,
       0,     0,   337,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   251,   252,     0,     0,     0,     0,     0,
       0,     0,     0,   253,   -96,   254,     0,     0,   255,     0,
       0,     0,     0,     0,   -96,     0,     0,     0,     0,   211,
     212,   213,   214,   215,   216,   217,   745,   261,   262,     0,
       0,     0,     0,     0,     0,   264,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   265,   266,   267,   268,   269,   270,
     271,   272,   273,   274,   275,   276,   277,   278,   279,   280,
     281,   282,   283,   284,   285,   286,   287,   288,   289,   290,
     291,   292,   293,   744,     0,   230,   241,   242,     0,   243,
      26,    34,   205,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   244,     0,     0,     0,    48,     0,
       0,     0,     0,     0,     0,     0,     0,   337,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   251,   252,
       0,     0,     0,     0,     0,     0,     0,     0,   253,     0,
     254,     0,     0,   255,     0,     0,     0,     0,     0,    49,
       0,     0,     0,     0,   211,   212,   213,   214,   215,   216,
     217,   745,   261,   262,     0,     0,     0,     0,     0,     0,
     264,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,   797,    98,
       0,     0,     0,   798,    34,   205,   100,   101,     0,   102,
       0,     0,   103,   104,   105,   106,   107,     0,     0,   108,
     109,     0,     0,   110,   111,   112,   113,   114,   115,     0,
     799,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   369,     0,   209,     0,     0,     0,     0,     0,   118,
     119,   120,     0,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   800,   142,   143,   144,   145,   146,   147,
     148,   149,   150,   151,   152,   153,   154,   155,   156,   157,
     158,   159,   160,   161,   162,   163,   164,   165,   166,   167,
     168,   169,   170,   171,   172,   173,   174,   175,   176,   177,
     178,   179,   180,   181,   182,   183,   184,   185,   186,   187,
     188,   189,   190,   191,   192,   193,   194,   195,   196,   197,
     198,    97,    98,     0,     0,     0,    99,     0,     0,   100,
     101,     0,   102,     0,     0,   103,   104,   105,   106,   107,
       0,     0,   108,   109,     0,     0,   110,   111,   112,   113,
     114,   115,     0,   116,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   117,     0,     0,     0,     0,     0,
       0,     0,   118,   119,   120,     0,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,   146,   147,   148,   149,   150,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   179,   180,   181,   182,   183,   184,
     185,   186,   187,   188,   189,   190,   191,   192,   193,   194,
     195,   196,   197,   198,    97,    98,     0,     0,     0,    99,
       0,     0,   100,   101,     0,   102,     0,     0,   103,   104,
     105,   106,   107,     0,     0,   108,   109,     0,     0,   110,
     111,   112,   113,   114,   115,     0,   116,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   118,   119,   120,     0,   121,
     122,   123,   124,   125,   126,   127,   128,   129,   130,   131,
     132,   133,   134,   135,   136,   137,   138,   139,   140,   141,
     142,   143,   144,   145,   146,   147,   148,   149,   150,   151,
     152,   153,   154,   155,   156,   157,   158,   159,   160,   161,
     162,   163,   164,   165,   166,   167,   168,   169,   170,   171,
     172,   173,   174,   175,   176,   177,   178,   179,   180,   181,
     182,   183,   184,   185,   186,   187,   188,   189,   190,   191,
     192,   193,   194,   195,   196,   197,   198,   230,   241,   242,
       0,   243,    26,    34,   205,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   244,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   337,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     251,   252,     0,     0,     0,     0,     0,     0,     0,   672,
     673,     0,   254,     0,     0,   255,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   889,   890,   891,   892,
     893,   894,   895,   896,   261,   262,     0,     0,     0,     0,
       0,     0,   264,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   286,   287,   288,   289,   290,   291,   292,   293,
     230,   241,   242,     0,   243,    26,    34,   205,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   244,
       0,     0,     0,     0,     0,   483,     0,     0,     0,     0,
       0,     0,   484,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   251,   252,     0,     0,     0,     0,     0,
       0,     0,     0,   253,     0,   254,     0,     0,   255,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   261,   262,   485,
       0,     0,     0,     0,     0,   264,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   265,   266,   267,   268,   269,   270,
     271,   272,   273,   274,   275,   276,   277,   278,   279,   280,
     281,   282,   283,   284,   285,   286,   287,   288,   289,   290,
     291,   292,   293,   230,   241,   242,     0,   243,    26,    34,
     205,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   244,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   337,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   251,   252,     0,     0,
       0,     0,   388,     0,     0,     0,   253,     0,   254,     0,
       0,   255,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     261,   262,   485,     0,     0,     0,     0,     0,   264,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   265,   266,   267,
     268,   269,   270,   271,   272,   273,   274,   275,   276,   277,
     278,   279,   280,   281,   282,   283,   284,   285,   286,   287,
     288,   289,   290,   291,   292,   293,   230,   241,   242,     0,
     243,    26,    34,   205,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   244,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   337,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   251,
     252,     0,     0,     0,     0,   388,     0,     0,     0,   253,
       0,   254,     0,     0,   255,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   261,   262,     0,     0,     0,     0,     0,
       0,   264,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     265,   266,   267,   268,   269,   270,   271,   272,   273,   274,
     275,   276,   277,   278,   279,   280,   281,   282,   283,   284,
     285,   286,   287,   288,   289,   290,   291,   292,   293,   230,
     241,   242,     0,   243,    26,    34,   205,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   244,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   337,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   251,   252,     0,     0,     0,     0,     0,     0,
       0,   672,   673,     0,   254,     0,     0,   255,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   261,   262,     0,     0,
       0,     0,     0,     0,   264,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   265,   266,   267,   268,   269,   270,   271,
     272,   273,   274,   275,   276,   277,   278,   279,   280,   281,
     282,   283,   284,   285,   286,   287,   288,   289,   290,   291,
     292,   293,   230,   241,   242,     0,   243,    26,    34,   205,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   244,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   816,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   251,   252,     0,     0,     0,
       0,   388,     0,     0,     0,   253,     0,   254,     0,     0,
     255,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   261,
     262,     0,     0,     0,     0,     0,     0,   264,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   265,   266,   267,   268,
     269,   270,   271,   272,   273,   274,   275,   276,   277,   278,
     279,   280,   281,   282,   283,   284,   285,   286,   287,   288,
     289,   290,   291,   292,   293,   230,   241,   242,     0,   243,
      26,    34,   205,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   244,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   337,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   251,   252,
       0,     0,     0,     0,     0,     0,     0,     0,   938,     0,
     254,     0,     0,   255,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   261,   262,   485,     0,     0,     0,     0,     0,
     264,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     286,   287,   288,   289,   290,   291,   292,   293,   230,   241,
     242,     0,   243,    26,    34,   205,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   244,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     337,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   251,   252,     0,     0,     0,     0,     0,     0,     0,
       0,   253,     0,   254,     0,     0,   255,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   261,   262,     0,     0,     0,
       0,     0,     0,   264,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,   286,   287,   288,   289,   290,   291,   292,
     293,   230,   241,   242,     0,   243,    26,    34,   205,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     244,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   337,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   251,   252,     0,     0,     0,     0,
       0,     0,     0,     0,   253,     0,   400,     0,     0,   255,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   261,   262,
       0,     0,     0,     0,     0,     0,   264,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   265,   266,   267,   268,   269,
     270,   271,   272,   273,   274,   275,   276,   277,   278,   279,
     280,   281,   282,   283,   284,   285,   286,   287,   288,   289,
     290,   291,   292,   293,   230,   241,   242,     0,   243,    26,
      34,   205,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   636,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   337,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   251,   252,     0,
       0,     0,     0,     0,     0,     0,     0,   637,     0,   254,
       0,     0,   255,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   261,   262,     0,     0,     0,     0,     0,     0,   264,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   265,   266,
     267,   268,   269,   270,   271,   272,   273,   274,   275,   276,
     277,   278,   279,   280,   281,   282,   283,   284,   285,   286,
     287,   288,   289,   290,   291,   292,   293
};
static const yytype_int16 yycheck[] =
{
       0,     3,     4,     5,     6,     7,     8,   313,    43,    11,
      45,   381,    69,   260,    14,   332,   443,   353,    78,    40,
      41,   553,    24,   723,    69,    39,    77,   727,    30,    69,
     251,   252,   673,    84,   255,   725,   726,   500,   451,   208,
      75,    76,    44,   776,   388,   671,   864,    67,   719,   381,
      50,   845,   846,    55,    18,    57,   926,    59,    13,    80,
      62,    63,    64,    65,   375,   376,     9,    69,    23,     0,
      72,    73,    27,   867,    26,    77,   446,   447,    26,   411,
       8,     9,    84,    13,   416,    12,    88,     1,   257,    47,
       5,     6,     7,     8,     8,    16,    11,    15,    27,   431,
      28,    15,     3,    16,     1,    26,    27,     8,     9,    10,
     980,     8,   448,    56,    27,    67,   452,     3,    15,    67,
     938,    22,     8,     9,    10,    83,     8,   945,    49,    44,
      59,   821,     8,     9,    35,    53,    57,    26,   871,    53,
      68,   874,    34,    23,   476,   939,    67,    27,    23,    35,
      26,    26,   463,   400,   465,    56,    53,    58,    26,    35,
      14,     8,     9,    10,    23,    27,    58,    26,    57,   987,
      56,    27,    58,   844,     8,     9,    10,   590,    67,     8,
       9,   975,    26,   203,    56,  1003,    15,    88,    35,    57,
     496,    67,    67,   228,    58,   989,   822,    59,   534,    67,
     369,    35,    88,    59,   218,    34,    27,   209,    67,    56,
     542,    58,    15,    57,   249,    27,   949,   561,   232,   233,
      52,    53,   224,    67,    58,   227,   926,   563,    13,    58,
      62,   931,    27,   933,    63,    22,    57,    49,     8,     9,
      10,    88,   244,   245,   246,   247,   467,   249,     8,     9,
     264,   253,    22,    13,    88,    15,    27,   259,     8,     9,
      10,   263,    26,    27,    59,    35,    22,    52,    53,   910,
     733,   734,    22,    27,    34,   965,   321,    62,    49,    49,
     980,   598,    13,    26,    13,    35,    56,    53,    58,   979,
       8,     9,   349,    57,   294,    26,    27,    15,    58,    49,
     706,   707,   708,    67,   349,    59,    56,    27,    58,   349,
     361,   313,   227,    27,    57,    46,    47,    27,    88,   636,
      27,    52,    53,   325,    67,    27,    57,   859,    27,    60,
      16,    62,   334,   335,   581,   582,    67,    57,    88,   341,
      58,    27,    49,   345,   379,    59,   348,   349,    15,    59,
     352,   365,   722,   413,    14,    22,   358,    59,    57,   361,
      26,    27,    27,    49,   366,   671,     4,     5,     6,     7,
       8,    27,   417,    11,    16,   681,   682,   417,   380,   381,
      22,    23,   417,   796,    26,    27,   388,   432,    27,   410,
     722,   391,   432,    59,    59,   564,   565,   432,    13,   812,
       3,    67,    13,    59,    27,   407,    44,    27,    27,   411,
     742,    27,    17,   415,   416,   417,    17,    55,    27,    57,
      59,    16,    64,    65,   730,    67,   839,   429,   430,    27,
     432,    69,    27,   468,    57,    46,    47,    57,    57,    77,
      78,    52,    53,    59,   446,   447,    84,    13,   861,   785,
      88,    62,   784,    15,    49,    34,   792,   789,    27,   795,
      22,    59,     8,     9,    63,   467,    15,    58,   637,    15,
     806,   803,    23,    22,   476,    27,    27,     8,     9,    10,
      49,    27,    27,   543,   705,   706,   707,   708,   490,    27,
      52,    53,    54,    55,   496,   552,   553,    28,    49,   912,
      62,    27,   929,   930,    35,    54,    55,    59,     8,     9,
     545,    27,    58,    59,    59,    15,   822,    58,    27,   946,
     580,    59,    63,     8,     9,    56,   586,    58,   775,    13,
      13,    57,    27,    49,    34,   549,    36,     8,     9,   541,
     542,    72,    73,    74,    75,    76,    77,    78,    57,    34,
      58,    23,    26,    27,    49,    27,   591,    88,    58,   561,
      23,   988,    46,    47,    27,   882,   883,    22,    52,    53,
      22,   209,   999,   986,     8,     9,    60,    49,    62,    58,
      15,    15,  1009,    16,    63,    59,    49,    22,   226,   227,
     604,   605,   761,    67,  1007,  1008,    26,    27,   600,    34,
      34,    36,    36,  1016,    15,    15,   244,   245,   246,   247,
      16,   249,   833,   834,   835,   836,   837,   838,    59,   840,
      34,    27,    16,    34,    58,    36,   873,   997,    15,    59,
      15,   645,   646,    27,   636,   637,    13,    67,    52,    53,
     642,     0,     1,    13,     8,     9,    23,    50,    51,     3,
      27,    58,    11,    12,     8,     9,    59,   702,    22,    18,
      19,    20,    21,    16,    16,    24,    25,    26,   974,   671,
      56,   673,     5,     6,    33,    27,    46,    47,    15,   681,
     682,    15,    52,    53,    13,    22,    15,   325,    22,    56,
      60,    23,    62,    22,    23,    27,   334,    22,    27,    27,
     702,    37,    38,    39,    40,    41,    42,    58,    67,   347,
     348,   349,    48,    49,   352,   353,    58,   719,     0,     1,
     722,    63,    23,   361,    34,    61,    27,    27,   730,   743,
      15,    13,    14,    15,    16,    17,    27,    22,    23,   739,
     742,    57,    27,   381,    26,    63,   746,    52,    53,    15,
      15,    58,    15,    15,    59,   755,    22,    22,    13,    22,
      15,     8,     9,    10,    46,    47,   404,    22,    23,   407,
      52,    53,    27,   411,     8,   413,    15,   779,   416,   781,
      23,    28,   784,    22,    27,    67,   800,   789,    35,    54,
      55,   429,    15,    58,   796,    56,    15,   811,    63,    22,
      46,   803,   859,    22,   251,   252,   863,    34,   255,    56,
     448,    58,    59,    23,   452,    50,    51,    27,   820,   819,
     822,    59,    64,    65,    46,    72,    73,    74,    75,    76,
      77,    78,    58,    23,    58,    23,   838,    27,   476,    27,
      58,    88,   844,     0,     1,    23,    55,   847,    13,    27,
      15,   886,    23,   867,    11,    12,    27,    22,    64,    65,
      27,    18,    19,    20,    21,    60,    63,    24,    25,    26,
     905,    23,    56,    23,    34,    27,    33,    27,    43,    44,
      45,    46,    47,   885,    50,    34,    34,    52,    53,    54,
      55,    34,    69,    58,    13,    60,   534,    62,    63,    39,
      40,    41,    42,   541,   542,   543,    46,    47,   910,    47,
      67,    23,    23,   927,   916,    27,    27,    23,    58,    58,
      34,    27,    23,    59,    23,   563,    34,    58,    22,    34,
      34,    82,     0,     1,   934,    23,   938,    24,   952,    30,
     323,   543,   580,    11,    12,   910,   960,   863,   586,   754,
      18,    19,    20,    21,   945,   969,    24,    25,    26,   987,
     962,   642,   600,   702,     1,    33,   541,   962,   349,   330,
     117,   882,   974,   636,    11,    12,   366,    13,   978,    15,
      -1,    18,    19,    20,    21,    -1,    22,    24,    25,    26,
      -1,     3,     4,     5,    -1,   997,    33,    -1,   636,    67,
      -1,    -1,  1002,    -1,   642,    -1,    13,    43,    15,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    52,    53,    54,    55,
      27,    -1,    58,    -1,    -1,    -1,    62,    63,    -1,    -1,
      67,   208,   209,    -1,    -1,   673,    43,    44,    45,    46,
      47,    -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,
      -1,    58,    59,    60,    15,    62,    63,    -1,    -1,    -1,
      -1,    22,    -1,    -1,   702,    -1,    73,   244,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   253,   254,    -1,   256,
     257,   719,    -1,   260,   722,   723,    -1,    -1,    -1,   727,
      -1,    52,    53,    54,    55,    -1,    -1,    58,    -1,    -1,
      -1,    62,    63,    -1,   742,   117,   118,   119,   120,   121,
     122,   123,   124,   125,   126,   127,   128,   129,   130,   131,
     132,   133,   134,   135,   136,   137,   138,   139,   140,   141,
     142,   143,   144,   145,    -1,    -1,    -1,    -1,   776,    -1,
      -1,   779,    -1,   781,   321,    -1,   784,   785,    -1,    -1,
      -1,   789,    -1,    -1,   792,    -1,    -1,   795,    -1,    -1,
      13,    -1,    15,    -1,    -1,   803,    -1,    -1,   806,    22,
     347,    -1,   349,    -1,    -1,    -1,   353,    -1,    -1,    -1,
      -1,    -1,    -1,    71,    -1,    73,    13,    -1,    15,    -1,
      43,    -1,   369,    -1,    -1,    22,    -1,    -1,    -1,    52,
      53,    54,    55,     8,   381,    -1,   844,    -1,    -1,    62,
      -1,   388,    -1,    -1,    -1,    -1,    43,    44,    45,    46,
      47,    -1,    -1,   400,    -1,    52,    53,    54,    55,    -1,
     407,    -1,    -1,   871,   411,    62,   874,   414,    -1,   416,
     417,   418,   419,   420,   421,   422,   423,   885,    -1,   426,
      -1,   428,    -1,    -1,   431,   432,   433,   434,   435,   436,
     437,   438,    -1,    -1,   441,    -1,    -1,    -1,    -1,    -1,
      -1,   448,   910,   450,   451,   452,    -1,    -1,   455,   456,
     457,   458,   459,   460,    -1,    -1,    -1,    -1,   926,    -1,
      -1,    -1,    -1,   931,    -1,   933,    -1,    -1,    -1,   476,
     477,   571,   572,   573,   574,   575,   576,   577,   578,   579,
      -1,   949,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,    -1,   980,    13,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   244,   534,    -1,    -1,
      -1,    -1,    -1,    -1,   541,   542,    -1,    37,    38,    39,
      40,    41,    42,    -1,    -1,    -1,    46,    47,    48,    49,
      -1,    -1,    52,    53,   561,    -1,   563,   564,   565,    -1,
      60,    61,    62,    -1,   571,   572,   573,   574,   575,   576,
     577,   578,   579,   580,   581,   582,    -1,    -1,    72,    73,
      -1,    -1,    -1,   590,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   600,   601,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    13,    -1,    15,    -1,    -1,    -1,   335,    -1,    -1,
      22,    -1,    -1,    -1,    -1,    -1,    -1,   345,    -1,   636,
     637,    -1,    -1,   351,    -1,   642,    -1,    -1,    -1,    -1,
      -1,    43,    44,    45,    46,    47,    -1,    -1,   366,   367,
      52,    53,    54,    55,    -1,    -1,    58,    -1,    60,    -1,
      62,    63,   380,   381,    -1,    -1,   673,    -1,    -1,    -1,
      -1,    73,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   411,    -1,   702,    -1,    -1,   416,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   427,
      -1,   429,    -1,   431,    -1,   722,   723,    -1,   725,   726,
     727,    -1,    -1,    -1,   442,    -1,    -1,    -1,   446,   447,
     224,   449,    -1,    -1,    -1,   742,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   461,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   761,    -1,    -1,   475,   476,    -1,
      -1,    13,   480,    15,   482,    -1,    -1,    -1,   775,    -1,
      22,    -1,   779,    -1,   781,    27,    -1,   784,   785,    -1,
      -1,    -1,   789,    -1,    -1,   792,    -1,    -1,   795,   796,
      -1,    43,    44,    45,    46,    47,   803,    -1,    -1,   806,
      52,    53,    54,    55,    -1,   812,    58,    59,    60,    -1,
      62,    63,    -1,    -1,   821,    -1,    -1,    -1,    -1,   313,
      -1,    -1,    -1,   541,   542,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   839,    -1,   552,   553,    -1,    -1,    -1,    -1,
      -1,   335,    -1,    -1,    -1,    -1,    -1,   341,    -1,    -1,
      -1,   345,    -1,    -1,   861,    -1,    -1,   864,    -1,    -1,
      -1,    -1,   580,    -1,   358,    -1,   873,    13,    -1,    15,
      -1,    -1,   366,    -1,    -1,    -1,    22,    -1,   885,    -1,
      26,    27,    -1,    -1,    -1,    -1,   380,   381,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    43,    44,    45,
      46,    47,    -1,    -1,    -1,   912,    52,    53,    54,    55,
      -1,    57,    -1,    -1,    60,    -1,    62,   411,   636,   926,
      -1,    67,    -1,    -1,   931,    -1,   933,    -1,    -1,    -1,
      -1,   938,    -1,    -1,    -1,    -1,   430,    -1,   945,    -1,
      -1,    -1,    78,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   446,   447,    -1,    -1,    -1,    -1,   965,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   979,   980,    -1,    -1,    -1,    -1,    -1,   986,
     987,    13,   476,    15,    -1,    -1,    -1,    -1,    -1,    -1,
      22,    -1,    -1,    -1,    -1,    -1,   490,    -1,    -1,    -1,
    1007,  1008,   496,    -1,   722,    -1,    -1,    -1,    -1,  1016,
     728,    43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,   742,    -1,    58,    -1,    60,    -1,
      62,    63,    -1,     3,     4,     5,   754,     7,     8,     9,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   552,   553,
      -1,    -1,    -1,    -1,    -1,    -1,   784,    -1,    -1,    -1,
      -1,   789,   208,    -1,    -1,    -1,    46,    47,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   803,    -1,    -1,    -1,    -1,
     226,    61,    -1,    -1,    -1,    13,    -1,    15,    16,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,
      80,    81,   248,    -1,    -1,    -1,    -1,   253,   254,    -1,
     256,   257,    -1,    -1,   260,    43,    44,    45,    46,    47,
      -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,    -1,
      -1,   859,    60,    -1,    62,   863,   864,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,   143,   144,   145,    -1,   671,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   321,    -1,   681,   682,    -1,
      13,    -1,    15,   911,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    26,    27,    -1,    -1,    -1,    -1,    -1,
      -1,   347,    -1,    -1,    -1,    -1,    13,   353,    15,    -1,
      43,    44,    45,    46,    47,    22,    -1,   945,   722,    52,
      53,    54,    55,   369,    57,    58,   730,    60,    -1,    62,
      63,    -1,    -1,    -1,    67,    -1,    43,    44,    45,    46,
      47,    -1,   388,    -1,    -1,    52,    53,    54,    55,    -1,
     754,    58,    -1,    -1,   400,    62,    63,    -1,   404,   987,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   413,   414,   997,
     998,    -1,   418,   419,   420,   421,   422,   423,    -1,    -1,
     426,    -1,   428,    -1,    -1,   431,    -1,   433,   434,   435,
     436,   437,   438,    -1,    13,   441,    15,    -1,    -1,    -1,
      -1,    -1,   448,    22,    -1,    -1,   452,    -1,    27,   455,
     456,   457,   458,   459,   460,    -1,   820,    -1,   822,    -1,
      -1,    -1,    -1,    -1,    43,    44,    45,    46,    47,    -1,
      -1,   477,    -1,    52,    53,    54,    55,    13,    -1,    15,
      59,    60,    -1,    62,    -1,    -1,    22,    23,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   859,    -1,    -1,    -1,   863,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    43,    44,    45,
      46,    47,    -1,    -1,    -1,    -1,    52,    53,    54,    55,
      -1,    -1,    -1,    -1,    60,    -1,    62,    -1,   534,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   543,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   552,   553,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   561,    -1,   563,   564,   565,
      -1,    -1,    -1,    -1,    -1,   571,   572,   573,   574,   575,
     576,   577,   578,   579,   580,   581,   582,    -1,    -1,    -1,
     586,    13,    -1,    15,    -1,    -1,    -1,    -1,    -1,    -1,
      22,   208,    -1,    -1,    -1,   601,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     974,    43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,    -1,    -1,    -1,    -1,    60,    -1,
      62,   637,    -1,   997,    -1,    -1,   253,    58,    -1,   256,
     257,    -1,    -1,   260,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    83,    84,    85,    86,    -1,   673,    89,    90,
      91,    92,    93,    94,    95,    96,    97,    98,    99,   100,
     101,   102,   103,   104,   105,   106,   107,   108,   109,   110,
     111,   112,   113,   114,   115,   116,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   723,    -1,    -1,
      -1,   727,    -1,    -1,    -1,    -1,   732,    -1,    -1,    -1,
     347,    -1,    -1,    -1,    -1,   352,   353,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   754,    -1,
      -1,    -1,   369,    -1,    -1,   761,    -1,    -1,    -1,    -1,
       8,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   775,
     776,   388,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   785,
      -1,    -1,    -1,   400,    -1,    -1,   792,    -1,    -1,   795,
     407,    39,    40,    41,    42,    -1,    -1,   414,    -1,    -1,
     806,    -1,    -1,   420,   421,   422,   423,    -1,    -1,   426,
      -1,   428,    -1,    -1,    -1,    -1,    -1,    -1,   435,   436,
     437,   438,    -1,    -1,   441,    -1,    -1,    -1,    -1,    -1,
      -1,   448,    -1,   450,   451,   452,    -1,    -1,   455,   456,
     457,   458,   459,   460,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   859,    -1,    -1,    -1,   863,   864,    -1,
     477,    -1,    -1,    -1,    -1,   871,    -1,   873,   874,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,   534,    -1,    -1,
     926,    -1,    -1,    -1,    -1,   931,    -1,   933,    -1,    -1,
      -1,    -1,   938,    -1,    -1,    -1,    -1,    -1,    -1,   945,
      35,    -1,    -1,   949,   561,    -1,   563,   564,   565,    -1,
      -1,    46,    47,    -1,   571,   572,   573,   574,   575,   576,
     577,   578,   579,    58,   581,   582,    61,    13,    -1,    15,
      -1,    -1,    -1,   590,   980,    -1,    22,    -1,    -1,    -1,
      -1,   987,    -1,    -1,   601,    80,    81,    -1,    -1,    -1,
      -1,    -1,    -1,    88,    -1,    -1,    -1,    43,    44,    45,
      46,    47,    -1,    -1,    -1,    -1,    52,    53,    54,    55,
      -1,    -1,    -1,    -1,    60,    -1,    62,    -1,    -1,    -1,
     637,    -1,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,    -1,    -1,    -1,    -1,    -1,   673,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,   143,   144,   145,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   723,    -1,   725,   726,
     727,    -1,    -1,    -1,    -1,   732,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   761,    -1,    -1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,   775,    -1,
      -1,    -1,   779,    -1,   781,    -1,    -1,    22,   785,    -1,
      -1,    26,    -1,    28,    -1,   792,    -1,    -1,   795,   796,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   806,
      -1,    46,    47,    -1,    -1,   812,    -1,    -1,    -1,    -1,
      -1,    56,    -1,    58,   821,    -1,    61,    -1,    -1,    -1,
      -1,    -1,    67,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   839,    -1,    -1,    80,    81,    82,    -1,    -1,
      -1,    -1,    -1,    88,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   861,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   873,    -1,    -1,    -1,
      -1,    -1,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,    -1,    -1,    -1,    -1,   912,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   926,
      -1,    -1,    -1,    -1,   931,    -1,   933,    -1,    -1,    -1,
      -1,   938,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,     1,    -1,     3,     4,     5,   965,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   979,   980,    22,    -1,    -1,    -1,    26,   986,
      28,    29,    30,    31,    32,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,
    1007,  1008,    50,    51,    -1,    -1,    -1,    -1,    56,  1016,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    66,    67,
      68,    69,    70,    71,    72,    73,    74,    75,    76,    77,
      78,    -1,    80,    81,    -1,    -1,    -1,    -1,    -1,    87,
      88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      23,    -1,    -1,    26,    -1,    28,    29,    30,    31,    32,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    46,    47,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    66,    67,    68,    69,    70,    71,    72,
      73,    74,    75,    76,    77,    78,    -1,    80,    81,    -1,
      -1,    -1,    -1,    -1,    87,    88,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
     143,   144,   145,     1,    -1,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    23,    -1,    -1,    26,    -1,
      28,    29,    30,    31,    32,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    66,    67,
      68,    69,    70,    71,    72,    73,    74,    75,    76,    77,
      78,    -1,    80,    81,    -1,    -1,    -1,    -1,    -1,    87,
      88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    26,    -1,    28,    29,    30,    31,    32,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    46,    47,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    66,    67,    68,    69,    70,    71,    72,
      73,    74,    75,    76,    77,    78,    -1,    80,    81,    -1,
      -1,    -1,    -1,    -1,    87,    88,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
     143,   144,   145,     1,    -1,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    26,    -1,
      28,    29,    30,    31,    32,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    66,    67,
      68,    69,    70,    71,    72,    73,    74,    75,    76,    77,
      78,    -1,    80,    81,    -1,    -1,    -1,    -1,    -1,    87,
      88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    26,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    46,    47,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    57,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    -1,    67,    -1,    -1,    -1,    -1,    72,
      73,    74,    75,    76,    77,    78,    79,    80,    81,    -1,
      -1,    -1,    -1,    -1,    -1,    88,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
     143,   144,   145,     1,    -1,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    26,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    67,
      -1,    -1,    -1,    -1,    72,    73,    74,    75,    76,    77,
      78,    79,    80,    81,    -1,    -1,    -1,    -1,    -1,    -1,
      88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,     3,     4,
      -1,    -1,    -1,     8,     9,    10,    11,    12,    -1,    14,
      -1,    -1,    17,    18,    19,    20,    21,    -1,    -1,    24,
      25,    -1,    -1,    28,    29,    30,    31,    32,    33,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    56,    -1,    58,    -1,    -1,    -1,    -1,    -1,    64,
      65,    66,    -1,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    77,    78,    79,    80,    81,    82,    83,    84,
      85,    86,    87,    88,    89,    90,    91,    92,    93,    94,
      95,    96,    97,    98,    99,   100,   101,   102,   103,   104,
     105,   106,   107,   108,   109,   110,   111,   112,   113,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,     3,     4,    -1,    -1,    -1,     8,    -1,    -1,    11,
      12,    -1,    14,    -1,    -1,    17,    18,    19,    20,    21,
      -1,    -1,    24,    25,    -1,    -1,    28,    29,    30,    31,
      32,    33,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    56,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    64,    65,    66,    -1,    68,    69,    70,    71,
      72,    73,    74,    75,    76,    77,    78,    79,    80,    81,
      82,    83,    84,    85,    86,    87,    88,    89,    90,    91,
      92,    93,    94,    95,    96,    97,    98,    99,   100,   101,
     102,   103,   104,   105,   106,   107,   108,   109,   110,   111,
     112,   113,   114,   115,   116,   117,   118,   119,   120,   121,
     122,   123,   124,   125,   126,   127,   128,   129,   130,   131,
     132,   133,   134,   135,   136,   137,   138,   139,   140,   141,
     142,   143,   144,   145,     3,     4,    -1,    -1,    -1,     8,
      -1,    -1,    11,    12,    -1,    14,    -1,    -1,    17,    18,
      19,    20,    21,    -1,    -1,    24,    25,    -1,    -1,    28,
      29,    30,    31,    32,    33,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    64,    65,    66,    -1,    68,
      69,    70,    71,    72,    73,    74,    75,    76,    77,    78,
      79,    80,    81,    82,    83,    84,    85,    86,    87,    88,
      89,    90,    91,    92,    93,    94,    95,    96,    97,    98,
      99,   100,   101,   102,   103,   104,   105,   106,   107,   108,
     109,   110,   111,   112,   113,   114,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,   143,   144,   145,     3,     4,     5,
      -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      46,    47,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    55,
      56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    72,    73,    74,    75,
      76,    77,    78,    79,    80,    81,    -1,    -1,    -1,    -1,
      -1,    -1,    88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   117,   118,   119,   120,   121,   122,   123,   124,   125,
     126,   127,   128,   129,   130,   131,   132,   133,   134,   135,
     136,   137,   138,   139,   140,   141,   142,   143,   144,   145,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    -1,    -1,    28,    -1,    -1,    -1,    -1,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    46,    47,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    80,    81,    82,
      -1,    -1,    -1,    -1,    -1,    88,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
     143,   144,   145,     3,     4,     5,    -1,     7,     8,     9,
      10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    46,    47,    -1,    -1,
      -1,    -1,    52,    -1,    -1,    -1,    56,    -1,    58,    -1,
      -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      80,    81,    82,    -1,    -1,    -1,    -1,    -1,    88,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,   143,   144,   145,     3,     4,     5,    -1,
       7,     8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,
      47,    -1,    -1,    -1,    -1,    52,    -1,    -1,    -1,    56,
      -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    80,    81,    -1,    -1,    -1,    -1,    -1,
      -1,    88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,   143,   144,   145,     3,
       4,     5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    46,    47,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    55,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    80,    81,    -1,    -1,
      -1,    -1,    -1,    -1,    88,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   117,   118,   119,   120,   121,   122,   123,
     124,   125,   126,   127,   128,   129,   130,   131,   132,   133,
     134,   135,   136,   137,   138,   139,   140,   141,   142,   143,
     144,   145,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    46,    47,    -1,    -1,    -1,
      -1,    52,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    80,
      81,    -1,    -1,    -1,    -1,    -1,    -1,    88,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,   143,   144,   145,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    80,    81,    82,    -1,    -1,    -1,    -1,    -1,
      88,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   143,   144,   145,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    46,    47,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    80,    81,    -1,    -1,    -1,
      -1,    -1,    -1,    88,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144,
     145,     3,     4,     5,    -1,     7,     8,     9,    10,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    46,    47,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    80,    81,
      -1,    -1,    -1,    -1,    -1,    -1,    88,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   117,   118,   119,   120,   121,
     122,   123,   124,   125,   126,   127,   128,   129,   130,   131,
     132,   133,   134,   135,   136,   137,   138,   139,   140,   141,
     142,   143,   144,   145,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    46,    47,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    80,    81,    -1,    -1,    -1,    -1,    -1,    -1,    88,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,   143,   144,   145
};
/* YYSTOS[STATE-NUM] -- The symbol kind of the accessing symbol of
   state STATE-NUM.  */
static const yytype_int16 yystos[] =
{
       0,     1,    11,    12,    18,    19,    20,    21,    24,    25,
      26,    33,    67,   153,   154,   155,   156,   162,   163,   164,
     165,   166,   167,   263,    12,     1,     8,    15,    53,   157,
     158,   159,   160,   230,     9,   161,   230,   231,   241,   242,
     161,   161,   161,   161,    18,   161,     0,   263,    26,    67,
     154,     1,   157,    15,    53,    13,   159,    14,     1,    15,
      16,    17,    46,    47,    52,    53,    56,   232,   233,    58,
     213,    15,    15,    22,   243,   213,   213,    22,    22,   171,
     161,   171,   155,   263,    13,    53,   240,   241,    14,   241,
     230,     5,     6,   230,   230,   230,   230,     3,     4,     8,
      11,    12,    14,    17,    18,    19,    20,    21,    24,    25,
      28,    29,    30,    31,    32,    33,    35,    56,    64,    65,
      66,    68,    69,    70,    71,    72,    73,    74,    75,    76,
      77,    78,    79,    80,    81,    82,    83,    84,    85,    86,
      87,    88,    89,    90,    91,    92,    93,    94,    95,    96,
      97,    98,    99,   100,   101,   102,   103,   104,   105,   106,
     107,   108,   109,   110,   111,   112,   113,   114,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,   143,   144,   145,   234,
     235,   238,   239,     9,   233,    10,    28,    35,    56,    58,
      59,    72,    73,    74,    75,    76,    77,    78,    88,   192,
     214,   215,   217,   221,   241,   253,   258,    13,   168,   169,
       3,   246,   231,   231,   244,   245,   246,   171,   171,   240,
       1,     4,     5,     7,    22,    28,    29,    30,    31,    32,
      35,    46,    47,    56,    58,    61,    66,    68,    69,    70,
      71,    80,    81,    87,    88,   117,   118,   119,   120,   121,
     122,   123,   124,   125,   126,   127,   128,   129,   130,   131,
     132,   133,   134,   135,   136,   137,   138,   139,   140,   141,
     142,   143,   144,   145,   172,   173,   174,   179,   186,   192,
     197,   198,   199,   200,   201,   202,   203,   212,   216,   217,
     221,   225,   230,   241,   246,   247,   248,   249,   250,   251,
     254,   258,   261,   213,   240,    27,   241,    17,    17,   234,
      27,    57,    13,   233,    34,    15,   243,    35,   192,   193,
     197,   241,   254,   192,   241,    15,   243,    58,    63,    27,
      59,    15,    34,    58,    63,   219,   231,   197,   241,   161,
     171,    63,   170,   243,   243,    23,    27,    13,    23,    56,
     190,   191,   192,   246,   241,   241,   241,   197,   171,   241,
      15,    22,    34,    36,   249,   261,   249,   261,    52,   192,
     193,   194,   196,   197,   230,   254,   192,   197,   249,   261,
      58,   192,   197,   228,   229,   254,   193,    58,   230,   229,
     230,    22,    23,   263,    13,    15,    22,    43,    44,    45,
      46,    47,    52,    53,    54,    55,    60,    62,    13,    15,
      16,    22,    43,    44,    45,    46,    47,    52,    53,    54,
      55,    60,    62,   205,   206,   207,    22,    22,    58,    15,
      34,    36,    58,   218,   219,    13,    46,    47,    52,    53,
      60,    62,   188,   189,   192,   197,   221,    16,   168,   241,
      57,   235,   238,   241,   246,    15,    22,    27,    57,    59,
      15,   246,    15,    28,    35,    82,   192,   197,   222,   224,
     241,   252,   254,   220,   241,   215,   246,   241,   254,   222,
      58,    83,    84,    85,    86,    89,    90,    91,    92,    93,
      94,    95,    96,    97,    98,    99,   100,   101,   102,   103,
     104,   105,   106,   107,   108,   109,   110,   111,   112,   113,
     114,   115,   116,   262,    58,   240,   243,   245,   246,   193,
      23,    27,    49,    22,   206,   206,   171,   230,   191,   231,
     244,   246,    56,    56,   192,   196,   197,   254,    57,    27,
      57,   195,   263,    58,    27,    27,    59,   192,   197,   229,
      73,    37,    38,    39,    40,    41,    42,    48,    49,    61,
      22,    50,    51,   171,   173,   227,   263,   192,   204,   254,
      34,   213,   191,   173,   192,   197,   254,   230,    22,   191,
      49,    56,   171,   192,   217,   221,   192,   197,   192,   197,
     192,   197,   254,   192,   197,   254,   192,   197,   254,   192,
     197,   254,   192,   197,   254,   246,   192,   197,   254,   230,
     241,   246,    28,    68,   210,   231,    22,    56,   191,   192,
     197,   246,    49,   171,   192,   217,   221,   192,   197,   192,
     197,   192,   197,   254,   192,   197,   254,   192,   197,   254,
     192,   197,   254,   192,   197,   254,   246,   207,   244,   244,
     222,   246,    55,    56,   175,   176,   192,   197,   254,   175,
     222,    27,    27,   192,   197,   254,   192,   197,   254,   192,
     197,   254,   192,   197,   254,   192,   197,   254,   192,   197,
     254,   246,    27,   206,   206,    39,    40,    41,    42,   230,
     255,   256,   261,   171,    57,   246,   192,   197,   254,    63,
     246,   246,    22,    27,    59,    13,    34,    58,    15,   219,
      59,   262,    34,    52,    53,   222,    57,   190,   191,   172,
     171,    34,    49,    23,     1,    79,   180,   181,   182,   197,
     231,   246,   258,   259,   260,   180,   196,   222,   193,   193,
      59,    56,   192,   228,   254,   228,   228,   228,   228,   228,
     228,   228,   228,   229,   229,    64,    65,   226,   173,    46,
      59,    46,   175,   171,    49,    58,   236,   237,   238,    49,
     187,   192,    58,   243,   243,    58,    58,     3,     8,    35,
      88,   236,   193,    49,    23,   187,    58,   243,   243,    23,
      23,    59,    34,   218,   219,    55,    35,   174,   177,   178,
     241,    27,    59,   219,   219,   188,   192,   221,   257,   261,
     257,   257,   257,    39,    40,    41,    42,    46,    47,    58,
      60,   220,   244,   224,    63,   176,   176,   223,   224,   246,
     219,    59,   197,   254,   262,   262,    59,    23,    23,    56,
     191,    34,    57,   263,    34,   182,    57,    59,    59,    59,
     193,   229,   227,    64,    65,   192,   254,   192,   254,   191,
     222,    23,    27,    13,   191,    27,    49,   222,   222,    72,
      73,    74,    75,    76,    77,    78,    79,   175,   208,   209,
     211,   230,    23,    57,   191,    49,   222,   243,   175,    57,
     263,    15,    34,   176,   218,   219,    47,   261,   261,   261,
     261,   261,   230,   261,   175,   261,    58,    23,   220,   205,
     205,    27,    59,   263,   180,    23,   175,   181,    56,   183,
     184,   192,   197,   246,   252,    34,   205,    57,   227,   229,
     227,    23,    59,    23,   237,   238,    23,   192,   171,    59,
      59,    59,    27,    59,    59,    34,    23,    23,   171,    59,
      35,   174,   177,   241,   246,   175,   230,    59,   223,    34,
      58,   224,   224,    57,   185,   252,    27,    49,   205,   183,
     227,   243,   243,   209,   230,   176,   243,    22,    15,   205,
      59,   176,   223,    27,    57,   175,   184,    27,    27,   205,
     244,   246,    59,   252,   175,   175,    27,    23,   175
};
/* YYR1[RULE-NUM] -- Symbol kind of the left-hand side of rule RULE-NUM.  */
static const yytype_int16 yyr1[] =
{
       0,   152,   153,   153,   153,   153,   153,   154,   154,   155,
     155,   155,   155,   155,   155,   155,   155,   156,   156,   156,
     156,   156,   156,   157,   157,   157,   157,   158,   158,   158,
     159,   159,   159,   159,   159,   159,   159,   160,   160,   160,
     160,   160,   160,   161,   161,   162,   162,   163,   164,   165,
     166,   167,   168,   169,   169,   170,   170,   171,   172,   172,
     172,   173,   173,   173,   173,   173,   173,   173,   173,   173,
     173,   173,   173,   173,   173,   174,   174,   174,   174,   175,
     175,   176,   176,   176,   176,   176,   177,   178,   178,   178,
     178,   179,   179,   179,   180,   180,   180,   181,   181,   181,
     181,   181,   181,   181,   181,   181,   182,   182,   182,   183,
     183,   184,   184,   184,   184,   184,   185,   185,   186,   186,
     186,   186,   186,   187,   187,   188,   188,   189,   189,   190,
     190,   190,   191,   191,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   193,   193,   193,   193,
     193,   193,   194,   194,   195,   195,   196,   196,   196,   196,
     196,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     197,   197,   197,   197,   197,   197,   197,   197,   197,   197,
     198,   199,   200,   201,   202,   202,   203,   204,   204,   204,
     204,   204,   204,   205,   205,   206,   206,   207,   207,   207,
     208,   208,   209,   210,   210,   210,   211,   211,   211,   211,
     211,   211,   211,   211,   207,   212,   213,   213,   213,   214,
     214,   215,   215,   215,   215,   215,   215,   215,   215,   215,
     215,   215,   215,   215,   216,   216,   216,   217,   217,   217,
     218,   218,   219,   219,   220,   220,   221,   221,   221,   222,
     222,   222,   223,   223,   223,   223,   224,   224,   224,   224,
     224,   224,   224,   224,   224,   224,   225,   225,   225,   225,
     226,   226,   227,   227,   227,   228,   228,   228,   229,   229,
     229,   229,   229,   229,   229,   229,   229,   229,   229,   229,
     229,   229,   230,   231,   231,   232,   232,   232,   233,   233,
     234,   234,   235,   235,   236,   236,   237,   237,   238,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     239,   239,   239,   239,   239,   239,   239,   239,   239,   239,
     240,   240,   240,   241,   242,   242,   243,   243,   243,   243,
     244,   244,   245,   245,   245,   245,   246,   247,   248,   249,
     249,   249,   250,   251,   251,   252,   253,   254,   254,   254,
     254,   254,   254,   254,   254,   254,   254,   254,   254,   254,
     254,   254,   255,   255,   255,   255,   255,   255,   255,   255,
     256,   256,   256,   256,   257,   257,   258,   258,   258,   258,
     258,   258,   258,   259,   260,   260,   261,   261,   261,   261,
     261,   261,   261,   261,   261,   261,   261,   261,   261,   261,
     261,   261,   261,   261,   261,   261,   261,   261,   261,   261,
     261,   261,   261,   261,   261,   262,   262,   262,   262,   262,
     262,   262,   262,   262,   262,   262,   262,   262,   262,   262,
     262,   262,   262,   262,   262,   262,   262,   262,   262,   262,
     262,   262,   262,   262,   262,   262,   262,   262,   262,   262,
     263,   263,   263,   263
};
/* YYR2[RULE-NUM] -- Number of symbols on the right-hand side of rule RULE-NUM.  */
static const yytype_int8 yyr2[] =
{
       0,     2,     1,     2,     2,     3,     1,     3,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     3,     5,     2,
       4,     2,     3,     1,     3,     2,     4,     1,     2,     3,
       1,     3,     3,     2,     4,     4,     2,     3,     3,     3,
       3,     3,     1,     3,     1,     5,     6,     4,     4,     5,
       3,     3,     2,     2,     0,     2,     0,     3,     3,     1,
       0,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     4,     2,     1,     3,     6,     3,     5,     3,
       1,     1,     1,     1,     3,     2,     4,     3,     3,     1,
       1,     5,     5,     7,     3,     1,     0,     3,     5,     4,
       6,     4,     5,     6,     7,     1,     1,     1,     1,     3,
       1,     1,     3,     1,     1,     1,     3,     1,     3,     3,
       1,     2,     2,     3,     1,     1,     1,     3,     1,     1,
       1,     3,     3,     1,     1,     3,     1,     2,     3,     4,
       1,     2,     3,     4,     1,     3,     3,     3,     3,     1,
       1,     1,     3,     1,     1,     1,     1,     1,     1,     5,
       2,     1,     1,     2,     2,     2,     2,     6,     6,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     4,     6,     6,     6,
       6,     3,     5,     4,     4,     3,     3,     4,     6,     6,
       6,     6,     3,     3,     3,     3,     5,     7,     7,     7,
       4,     4,     4,     4,     3,     6,     3,     6,     5,     5,
       5,     3,     4,     2,     3,     2,     4,     1,     1,     3,
       3,     3,     3,     1,     2,     0,     1,     2,     5,     5,
       3,     1,     3,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     5,     4,     3,     2,     0,     3,
       1,     1,     3,     1,     3,     3,     3,     2,     2,     1,
       1,     3,     5,     5,     2,     4,     5,     2,     4,     5,
       3,     3,     1,     4,     1,     3,     6,     9,     8,     3,
       1,     0,     3,     3,     1,     0,     1,     1,     1,     1,
       1,     3,     4,     3,     4,     6,     3,     5,     4,     6,
       3,     4,     1,     2,     1,     1,     1,     1,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     5,
       3,     1,     1,     1,     2,     1,     2,     3,     3,     5,
       1,     3,     1,     3,     1,     3,     1,     3,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       3,     2,     1,     1,     1,     2,     2,     3,     3,     4,
       1,     3,     1,     1,     3,     2,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     3,     3,     5,     6,     3,     2,     2,     2,     2,
       2,     2,     1,     3,     3,     3,     3,     3,     3,     3,
       2,     2,     2,     2,     1,     3,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     3,     3,     3,
       1,     1,     2,     2
};
/* YYDPREC[RULE-NUM] -- Dynamic precedence of rule #RULE-NUM (0 if none).  */
static const yytype_int8 yydprec[] =
{
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     1,     2,     1,
       2,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     2,     1,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     2,     2,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     1,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       1,     0,     0,     2,     2,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0
};
/* YYMERGER[RULE-NUM] -- Index of merging function for rule #RULE-NUM.  */
static const yytype_int8 yymerger[] =
{
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0
};
/* YYIMMEDIATE[RULE-NUM] -- True iff rule #RULE-NUM is not to be deferred, as
   in the case of predicates.  */
static const yybool yyimmediate[] =
{
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0
};
/* YYCONFLP[YYPACT[STATE-NUM]] -- Pointer into YYCONFL of start of
   list of conflicting reductions corresponding to action entry for
   state STATE-NUM in yytable.  0 means no conflicts.  The list in
   yyconfl is terminated by a rule number of 0.  */
static const yytype_int8 yyconflp[] =
{
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,    89,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   125,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     5,
       0,     0,     0,     0,     7,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    75,     0,    77,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      21,    23,     0,     0,     0,     0,    45,    25,    79,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    47,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      27,     0,     0,     0,     0,     0,     0,     0,     1,     0,
       0,     0,     0,     0,     0,     3,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    61,     0,     0,     0,     0,     0,
      63,    65,     0,     0,    67,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,    69,    71,     0,    73,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    31,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,    33,     0,     0,    15,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      13,     0,     0,     0,     0,     0,     0,     0,    17,     0,
     105,     0,     0,     0,   107,     0,     0,     9,     0,     0,
      11,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   121,     0,    19,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   123,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   109,     0,     0,     0,
     111,    29,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   101,   103,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   113,     0,     0,     0,   115,     0,
       0,     0,     0,     0,     0,     0,     0,    43,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      83,     0,     0,     0,     0,     0,     0,    85,     0,     0,
       0,     0,    87,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   117,     0,
       0,     0,   119,     0,    81,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    91,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    55,    57,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    59,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    39,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    41,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      93,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    95,    97,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    99,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    35,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    37,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    49,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      51,    53,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0
};
/* YYCONFL[I] -- lists of conflicting rule numbers, each terminated by
   0, pointed into by YYCONFLP.  */
static const short yyconfl[] =
{
       0,   503,     0,   503,     0,   134,     0,   134,     0,   161,
       0,   162,     0,   383,     0,   134,     0,   134,     0,   530,
       0,   134,     0,   134,     0,   134,     0,   134,     0,   332,
       0,   134,     0,   134,     0,   157,     0,   157,     0,   156,
       0,   156,     0,   383,     0,   158,     0,   158,     0,   367,
       0,   367,     0,   367,     0,   381,     0,   381,     0,   381,
       0,   162,     0,   162,     0,   162,     0,   162,     0,   162,
       0,   162,     0,   162,     0,   134,     0,   134,     0,   134,
       0,   135,     0,   504,     0,   504,     0,   512,     0,   513,
       0,   383,     0,   366,     0,   366,     0,   366,     0,   366,
       0,   365,     0,   365,     0,   516,     0,   516,     0,   399,
       0,   399,     0,   418,     0,   418,     0,   433,     0,   433,
       0,   134,     0,   134,     0,   383,     0
};
/* YYLLOC_DEFAULT -- Set CURRENT to span from RHS[1] to RHS[N].
   If N is 0, then set CURRENT to the empty location which ends
   the previous symbol: RHS[0] (always defined).  */
#ifndef YYLLOC_DEFAULT
# define YYLLOC_DEFAULT(Current, Rhs, N)                                \
    do                                                                  \
      if (N)                                                            \
        {                                                               \
          (Current).first_line   = YYRHSLOC (Rhs, 1).first_line;        \
          (Current).first_column = YYRHSLOC (Rhs, 1).first_column;      \
          (Current).last_line    = YYRHSLOC (Rhs, N).last_line;         \
          (Current).last_column  = YYRHSLOC (Rhs, N).last_column;       \
        }                                                               \
      else                                                              \
        {                                                               \
          (Current).first_line   = (Current).last_line   =              \
            YYRHSLOC (Rhs, 0).last_line;                                \
          (Current).first_column = (Current).last_column =              \
            YYRHSLOC (Rhs, 0).last_column;                              \
        }                                                               \
    while (0)
#endif
# define YYRHSLOC(Rhs, K) ((Rhs)[K].yystate.yyloc)
#undef yynerrs
#define yynerrs (yystackp->yyerrcnt)
#undef yychar
#define yychar (yystackp->yyrawchar)
#undef yylval
#define yylval (yystackp->yyval)
#undef yylloc
#define yylloc (yystackp->yyloc)
#define mca_nerrs yynerrs
#define mca_char yychar
#define mca_lval yylval
#define mca_lloc yylloc
enum { YYENOMEM = -2 };
typedef enum { yyok, yyaccept, yyabort, yyerr, yynomem } YYRESULTTAG;
#define YYCHK(YYE)                              \
  do {                                          \
    YYRESULTTAG yychk_flag = YYE;               \
    if (yychk_flag != yyok)                     \
      return yychk_flag;                        \
  } while (0)
/* YYINITDEPTH -- initial size of the parser's stacks.  */
#ifndef YYINITDEPTH
# define YYINITDEPTH 200
#endif
/* YYMAXDEPTH -- maximum size the stacks can grow to (effective only
   if the built-in stack extension method is used).
   Do not make this value too large; the results are undefined if
   SIZE_MAX < YYMAXDEPTH * sizeof (GLRStackItem)
   evaluated with infinite-precision integer arithmetic.  */
#ifndef YYMAXDEPTH
# define YYMAXDEPTH 10000
#endif
/* Minimum number of free items on the stack allowed after an
   allocation.  This is to allow allocation and initialization
   to be completed by functions that call yyexpandGLRStack before the
   stack is expanded, thus insuring that all necessary pointers get
   properly redirected to new data.  */
#define YYHEADROOM 2
#ifndef YYSTACKEXPANDABLE
#  define YYSTACKEXPANDABLE 1
#endif
#if YYSTACKEXPANDABLE
# define YY_RESERVE_GLRSTACK(Yystack)                   \
  do {                                                  \
    if (Yystack->yyspaceLeft < YYHEADROOM)              \
      yyexpandGLRStack (Yystack);                       \
  } while (0)
#else
# define YY_RESERVE_GLRSTACK(Yystack)                   \
  do {                                                  \
    if (Yystack->yyspaceLeft < YYHEADROOM)              \
      yyMemoryExhausted (Yystack);                      \
  } while (0)
#endif
/** State numbers. */
typedef int yy_state_t;
/** Rule numbers. */
typedef int yyRuleNum;
/** Item references. */
typedef short yyItemNum;
typedef struct yyGLRState yyGLRState;
typedef struct yyGLRStateSet yyGLRStateSet;
typedef struct yySemanticOption yySemanticOption;
typedef union yyGLRStackItem yyGLRStackItem;
typedef struct yyGLRStack yyGLRStack;
struct yyGLRState
{
  /** Type tag: always true.  */
  yybool yyisState;
  /** Type tag for yysemantics.  If true, yyval applies, otherwise
   *  yyfirstVal applies.  */
  yybool yyresolved;
  /** Number of corresponding LALR(1) machine state.  */
  yy_state_t yylrState;
  /** Preceding state in this stack */
  yyGLRState* yypred;
  /** Source position of the last token produced by my symbol */
  YYPTRDIFF_T yyposn;
  union {
    /** First in a chain of alternative reductions producing the
     *  nonterminal corresponding to this state, threaded through
     *  yynext.  */
    yySemanticOption* yyfirstVal;
    /** Semantic value for this state.  */
    YYSTYPE yyval;
  } yysemantics;
  /** Source location for this state.  */
  YYLTYPE yyloc;
};
struct yyGLRStateSet
{
  yyGLRState** yystates;
  /** During nondeterministic operation, yylookaheadNeeds tracks which
   *  stacks have actually needed the current lookahead.  During deterministic
   *  operation, yylookaheadNeeds[0] is not maintained since it would merely
   *  duplicate yychar != MCA_EMPTY.  */
  yybool* yylookaheadNeeds;
  YYPTRDIFF_T yysize;
  YYPTRDIFF_T yycapacity;
};
struct yySemanticOption
{
  /** Type tag: always false.  */
  yybool yyisState;
  /** Rule number for this reduction */
  yyRuleNum yyrule;
  /** The last RHS state in the list of states to be reduced.  */
  yyGLRState* yystate;
  /** The lookahead for this reduction.  */
  int yyrawchar;
  YYSTYPE yyval;
  YYLTYPE yyloc;
  /** Next sibling in chain of options.  To facilitate merging,
   *  options are chained in decreasing order by address.  */
  yySemanticOption* yynext;
};
/** Type of the items in the GLR stack.  The yyisState field
 *  indicates which item of the union is valid.  */
union yyGLRStackItem {
  yyGLRState yystate;
  yySemanticOption yyoption;
};
struct yyGLRStack {
  int yyerrState;
  /* To compute the location of the error token.  */
  yyGLRStackItem yyerror_range[3];
  int yyerrcnt;
  int yyrawchar;
  YYSTYPE yyval;
  YYLTYPE yyloc;
  YYJMP_BUF yyexception_buffer;
  yyGLRStackItem* yyitems;
  yyGLRStackItem* yynextFree;
  YYPTRDIFF_T yyspaceLeft;
  yyGLRState* yysplitPoint;
  yyGLRState* yylastDeleted;
  yyGLRStateSet yytops;
};
#if YYSTACKEXPANDABLE
static void yyexpandGLRStack (yyGLRStack* yystackp);
#endif
_Noreturn static void
yyFail (yyGLRStack* yystackp, YYLTYPE *yylocp, mc_value* mcast, const char* yymsg)
{
  if (yymsg != YY_NULLPTR)
    yyerror (yylocp, mcast, yymsg);
  YYLONGJMP (yystackp->yyexception_buffer, 1);
}
_Noreturn static void
yyMemoryExhausted (yyGLRStack* yystackp)
{
  YYLONGJMP (yystackp->yyexception_buffer, 2);
}
/** Accessing symbol of state YYSTATE.  */
static inline yysymbol_kind_t
yy_accessing_symbol (yy_state_t yystate)
{
  return YY_CAST (yysymbol_kind_t, yystos[yystate]);
}
#if MCA_DEBUG || 0
/* The user-facing name of the symbol whose (internal) number is
   YYSYMBOL.  No bounds checking.  */
static const char *yysymbol_name (yysymbol_kind_t yysymbol) YY_ATTRIBUTE_UNUSED;
/* YYTNAME[SYMBOL-NUM] -- String name of the symbol SYMBOL-NUM.
   First, the terminals, then, starting at YYNTOKENS, nonterminals.  */
static const char *const yytname[] =
{
  "\"end of file\"", "error", "\"invalid token\"", "MCTP_NUMBER_DEC",
  "MCTP_NUMBER_HEX", "MCTP_NUMBER_FLOAT", "MCTP_VERSION", "MCTP_STRING",
  "MCTP_ID", "MCTP_IDA", "MCOP_UNDERSCORE", "MCK_PUB", "MCK_USE",
  "MCPT_COLON", "MCK_AS", "MCPT_DOT", "MCPT_AT", "MCK_MC", "MCK_COMPONENT",
  "MCK_MODULE", "MCK_INTERFACE", "MCK_ENUM", "MCPT_LCURLY", "MCPT_RCURLY",
  "MCK_RECIPE", "MCK_ABSTRACT", "MCPT_SEMICOLON", "MCPT_COMMA", "MCK_ROLE",
  "MCK_CONDUIT", "MCK_DOMAIN", "MCK_RAIL", "MCK_PARTITION", "MCK_BOM",
  "MCOP_EQUAL", "MCK_PINS", "MCOP_PLUSEQUAL", "MCOP_EQUALEQUAL",
  "MCOP_NOTEQUAL", "MCOP_LESSTHAN", "MCOP_GREATERTHAN", "MCOP_LESSEQTHAN",
  "MCOP_GREATEREQTHAN", "MCOP_DOUBLEARROW", "MCOP_LEFTARROW",
  "MCOP_RIGHTARROW", "MCOP_PLUS", "MCOP_MINUS", "MCOP_AND", "MCOP_OR",
  "MCOP_ANDAND", "MCOP_OROR", "MCOP_MULTI", "MCOP_DIVID", "MCOP_CARET",
  "MCOP_APOST", "MCPT_LBRACKET", "MCPT_RBRACKET", "MCPT_LPAREN",
  "MCPT_RPAREN", "MCOP_TILDE", "MCOP_PLUSMINUS", "MCOP_TIMES",
  "MCPT_DBCOLON", "MCK_ELSE_IF", "MCK_ELSE", "MCK_IF", "MC_ENDL",
  "MCK_RETURN", "MCK_ERROR", "MCK_LET", "MCK_REQUIRE", "MCK_IO", "MCK_IN",
  "MCK_OUT", "MCK_NC", "MCK_PSRC", "MCK_PSNK", "MCK_PSBI", "MCK_RF",
  "MCONST_HIGH", "MCONST_LOW", "MCONST_NC", "MCU_INT", "MCU_HEX",
  "MCU_FLOAT", "MCU_STRING", "MCK_FUNC", "MCK_THIS", "MCU_VOLT", "MCU_AMP",
  "MCU_CAP", "MCU_IND", "MCU_TIME", "MCU_LEN", "MCU_WATT", "MCU_OHM",
  "MCU_TEMP", "MCU_HZ", "MCU_DB", "MCU_PPM", "MCU_PERCENT", "MCU_BAUD",
  "MCU_DATASIZE", "MCU_SPS", "MCU_SIEMENS", "MCU_RESPONSIVITY",
  "MCU_ANGLE", "MCU_ANGULAR_RATE", "MCU_ENERGY", "MCU_EFIELD",
  "MCU_HFIELD", "MCU_FLUX", "MCU_BFIELD", "MCU_SLEW", "MCU_NOISE",
  "MCU_CHARGE", "MCUVAL_VOLT", "MCUVAL_AMP", "MCUVAL_CAP", "MCUVAL_IND",
  "MCUVAL_TIME", "MCUVAL_LEN", "MCUVAL_WATT", "MCUVAL_OHM", "MCUVAL_TEMP",
  "MCUVAL_HZ", "MCUVAL_DB", "MCUVAL_PPM", "MCUVAL_PERCENT", "MCUVAL_BAUD",
  "MCUVAL_DATASIZE", "MCUVAL_SPS", "MCUVAL_SIEMENS", "MCUVAL_RESPONSIVITY",
  "MCUVAL_ANGLE", "MCUVAL_ANGULAR_RATE", "MCUVAL_ENERGY", "MCUVAL_EFIELD",
  "MCUVAL_HFIELD", "MCUVAL_FLUX", "MCUVAL_BFIELD", "MCUVAL_SLEW",
  "MCUVAL_NOISE", "MCUVAL_CHARGE", "MCUVAL_COMPOSITE", "MC_WS",
  "MC_SINGLE_COMMENT", "MC_MULTI_COMMENT", "UVALUE_PAIR_PREC",
  "UVALUE_WORD_PREC", "IDA_BASE_PREC", "$accept", "start", "mc_tops",
  "mc_top", "mc_use", "mc_uri", "mc_prefix", "mc_uri_trunk", "mc_levels",
  "mc_class_name", "mc_component", "mc_module", "mc_interface", "mc_enum",
  "mc_bom", "mc_recipe", "mc_component_derivs", "mc_component_variant",
  "mc_component_adopts", "mc_body", "mc_clauses", "mc_clause",
  "mc_attribute", "mc_attr_values", "mc_attr_value", "mc_attr_line_t",
  "mc_attr_lines", "mc_attribute_pin", "mc_pins_lines", "mc_pins_line",
  "mc_pin_idn", "mc_pins_names", "mc_pins_name", "mc_nc_list", "mc_net",
  "mc_opds", "mc_net_port_elem", "mc_net_port_elems", "mc_mn_opd",
  "mc_mn_opds", "mc_opd", "mc_phrases", "mc_list_items", "mc_list_sep",
  "mc_list_item", "mc_phrase", "mc_role", "mc_conduit", "mc_domain",
  "mc_rail", "mc_partition", "mc_error_stmt", "mc_error_msg", "mc_tattrs",
  "mc_tattrs_opt", "mc_tattr", "mc_tattr_args", "mc_tattr_arg", "mc_tkey",
  "mc_resword", "mc_function", "mc_paramds", "mc_pards", "mc_pard",
  "mc_declare_a", "mc_declare_a1", "mc_insts", "mc_inst", "mc_iface_name",
  "mc_declare_b", "mc_params", "mc_body_params", "mc_param", "mc_conds",
  "mc_conds_elifs", "mc_cond_block", "mc_expr", "mc_judge", "mc_id",
  "mc_ida", "mc_ida_tail", "mc_ida_bracket", "mc_ida_items", "mc_ida_item",
  "mc_reorder_items", "mc_reorder_item", "mc_ida_run", "mc_ida_run_atom",
  "mc_idss", "mc_ids", "mc_idseg", "mc_idm", "mc_idans", "mc_idan",
  "mc_int", "mc_hex", "mc_float", "mc_number", "mc_string", "mc_const",
  "mc_nc", "mc_underscore", "mc_literal", "mc_cond_word", "mc_cond_rel",
  "mc_cond_rel_head", "mc_iotype", "mc_iotype_rf", "mc_pin_iotype",
  "mc_unit_value", "mc_unit_type", "mc_endls", YY_NULLPTR
};
static const char *
yysymbol_name (yysymbol_kind_t yysymbol)
{
  return yytname[yysymbol];
}
#endif
/** Left-hand-side symbol for rule #YYRULE.  */
static inline yysymbol_kind_t
yylhsNonterm (yyRuleNum yyrule)
{
  return YY_CAST (yysymbol_kind_t, yyr1[yyrule]);
}
#if MCA_DEBUG
# ifndef YYFPRINTF
#  define YYFPRINTF fprintf
# endif
# define YY_FPRINTF                             \
  YY_IGNORE_USELESS_CAST_BEGIN YY_FPRINTF_
# define YY_FPRINTF_(Args)                      \
  do {                                          \
    YYFPRINTF Args;                             \
    YY_IGNORE_USELESS_CAST_END                  \
  } while (0)
# define YY_DPRINTF                             \
  YY_IGNORE_USELESS_CAST_BEGIN YY_DPRINTF_
# define YY_DPRINTF_(Args)                      \
  do {                                          \
    if (yydebug)                                \
      YYFPRINTF Args;                           \
    YY_IGNORE_USELESS_CAST_END                  \
  } while (0)
/* YYLOCATION_PRINT -- Print the location on the stream.
   This macro was not mandated originally: define only if we know
   we won't break user code: when these are the locations we know.  */
# ifndef YYLOCATION_PRINT
#  if defined YY_LOCATION_PRINT
   /* Temporary convenience wrapper in case some people defined the
      undocumented and private YY_LOCATION_PRINT macros.  */
#   define YYLOCATION_PRINT(File, Loc)  YY_LOCATION_PRINT(File, *(Loc))
#  elif defined MCA_LTYPE_IS_TRIVIAL && MCA_LTYPE_IS_TRIVIAL
/* Print *YYLOCP on YYO.  Private, do not rely on its existence. */
YY_ATTRIBUTE_UNUSED
static int
yy_location_print_ (FILE *yyo, YYLTYPE const * const yylocp)
{
  int res = 0;
  int end_col = 0 != yylocp->last_column ? yylocp->last_column - 1 : 0;
  if (0 <= yylocp->first_line)
    {
      res += YYFPRINTF (yyo, "%d", yylocp->first_line);
      if (0 <= yylocp->first_column)
        res += YYFPRINTF (yyo, ".%d", yylocp->first_column);
    }
  if (0 <= yylocp->last_line)
    {
      if (yylocp->first_line < yylocp->last_line)
        {
          res += YYFPRINTF (yyo, "-%d", yylocp->last_line);
          if (0 <= end_col)
            res += YYFPRINTF (yyo, ".%d", end_col);
        }
      else if (0 <= end_col && yylocp->first_column < end_col)
        res += YYFPRINTF (yyo, "-%d", end_col);
    }
  return res;
}
#   define YYLOCATION_PRINT  yy_location_print_
    /* Temporary convenience wrapper in case some people defined the
       undocumented and private YY_LOCATION_PRINT macros.  */
#   define YY_LOCATION_PRINT(File, Loc)  YYLOCATION_PRINT(File, &(Loc))
#  else
#   define YYLOCATION_PRINT(File, Loc) ((void) 0)
    /* Temporary convenience wrapper in case some people defined the
       undocumented and private YY_LOCATION_PRINT macros.  */
#   define YY_LOCATION_PRINT  YYLOCATION_PRINT
#  endif
# endif /* !defined YYLOCATION_PRINT */
/*-----------------------------------.
| Print this symbol's value on YYO.  |
`-----------------------------------*/
static void
yy_symbol_value_print (FILE *yyo,
                       yysymbol_kind_t yykind, YYSTYPE const * const yyvaluep, YYLTYPE const * const yylocationp, mc_value* mcast)
{
  FILE *yyoutput = yyo;
  YY_USE (yyoutput);
  YY_USE (yylocationp);
  YY_USE (mcast);
  if (!yyvaluep)
    return;
  YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN
  YY_USE (yykind);
  YY_IGNORE_MAYBE_UNINITIALIZED_END
}
/*---------------------------.
| Print this symbol on YYO.  |
`---------------------------*/
static void
yy_symbol_print (FILE *yyo,
                 yysymbol_kind_t yykind, YYSTYPE const * const yyvaluep, YYLTYPE const * const yylocationp, mc_value* mcast)
{
  YYFPRINTF (yyo, "%s %s (",
             yykind < YYNTOKENS ? "token" : "nterm", yysymbol_name (yykind));
  YYLOCATION_PRINT (yyo, yylocationp);
  YYFPRINTF (yyo, ": ");
  yy_symbol_value_print (yyo, yykind, yyvaluep, yylocationp, mcast);
  YYFPRINTF (yyo, ")");
}
# define YY_SYMBOL_PRINT(Title, Kind, Value, Location)                  \
  do {                                                                  \
    if (yydebug)                                                        \
      {                                                                 \
        YY_FPRINTF ((stderr, "%s ", Title));                            \
        yy_symbol_print (stderr, Kind, Value, Location, mcast);        \
        YY_FPRINTF ((stderr, "\n"));                                    \
      }                                                                 \
  } while (0)
static inline void
yy_reduce_print (yybool yynormal, yyGLRStackItem* yyvsp, YYPTRDIFF_T yyk,
                 yyRuleNum yyrule, mc_value* mcast);
# define YY_REDUCE_PRINT(Args)          \
  do {                                  \
    if (yydebug)                        \
      yy_reduce_print Args;             \
  } while (0)
/* Nonzero means print parse trace.  It is left uninitialized so that
   multiple parsers can coexist.  */
int yydebug;
static void yypstack (yyGLRStack* yystackp, YYPTRDIFF_T yyk)
  YY_ATTRIBUTE_UNUSED;
static void yypdumpstack (yyGLRStack* yystackp)
  YY_ATTRIBUTE_UNUSED;
#else /* !MCA_DEBUG */
# define YY_DPRINTF(Args) do {} while (yyfalse)
# define YY_SYMBOL_PRINT(Title, Kind, Value, Location)
# define YY_REDUCE_PRINT(Args)
#endif /* !MCA_DEBUG */
/** Fill in YYVSP[YYLOW1 .. YYLOW0-1] from the chain of states starting
 *  at YYVSP[YYLOW0].yystate.yypred.  Leaves YYVSP[YYLOW1].yystate.yypred
 *  containing the pointer to the next state in the chain.  */
static void yyfillin (yyGLRStackItem *, int, int) YY_ATTRIBUTE_UNUSED;
static void
yyfillin (yyGLRStackItem *yyvsp, int yylow0, int yylow1)
{
  int i;
  yyGLRState *s = yyvsp[yylow0].yystate.yypred;
  for (i = yylow0-1; i >= yylow1; i -= 1)
    {
#if MCA_DEBUG
      yyvsp[i].yystate.yylrState = s->yylrState;
#endif
      yyvsp[i].yystate.yyresolved = s->yyresolved;
      if (s->yyresolved)
        yyvsp[i].yystate.yysemantics.yyval = s->yysemantics.yyval;
      else
        /* The effect of using yyval or yyloc (in an immediate rule) is
         * undefined.  */
        yyvsp[i].yystate.yysemantics.yyfirstVal = YY_NULLPTR;
      yyvsp[i].yystate.yyloc = s->yyloc;
      s = yyvsp[i].yystate.yypred = s->yypred;
    }
}
/** If yychar is empty, fetch the next token.  */
static inline yysymbol_kind_t
yygetToken (int *yycharp, yyGLRStack* yystackp, mc_value* mcast)
{
  yysymbol_kind_t yytoken;
  YY_USE (mcast);
  if (*yycharp == MCA_EMPTY)
    {
      YY_DPRINTF ((stderr, "Reading a token\n"));
      *yycharp = yylex (&yylval, &yylloc);
    }
  if (*yycharp <= MCA_EOF)
    {
      *yycharp = MCA_EOF;
      yytoken = YYSYMBOL_YYEOF;
      YY_DPRINTF ((stderr, "Now at end of input.\n"));
    }
  else
    {
      yytoken = YYTRANSLATE (*yycharp);
      YY_SYMBOL_PRINT ("Next token is", yytoken, &yylval, &yylloc);
    }
  return yytoken;
}
/* Do nothing if YYNORMAL or if *YYLOW <= YYLOW1.  Otherwise, fill in
 * YYVSP[YYLOW1 .. *YYLOW-1] as in yyfillin and set *YYLOW = YYLOW1.
 * For convenience, always return YYLOW1.  */
static inline int yyfill (yyGLRStackItem *, int *, int, yybool)
     YY_ATTRIBUTE_UNUSED;
static inline int
yyfill (yyGLRStackItem *yyvsp, int *yylow, int yylow1, yybool yynormal)
{
  if (!yynormal && yylow1 < *yylow)
    {
      yyfillin (yyvsp, *yylow, yylow1);
      *yylow = yylow1;
    }
  return yylow1;
}
/** Perform user action for rule number YYN, with RHS length YYRHSLEN,
 *  and top stack item YYVSP.  YYLVALP points to place to put semantic
 *  value ($$), and yylocp points to place for location information
 *  (@$).  Returns yyok for normal return, yyaccept for YYACCEPT,
 *  yyerr for YYERROR, yyabort for YYABORT, yynomem for YYNOMEM.  */
static YYRESULTTAG
yyuserAction (yyRuleNum yyrule, int yyrhslen, yyGLRStackItem* yyvsp,
              yyGLRStack* yystackp, YYPTRDIFF_T yyk,
              YYSTYPE* yyvalp, YYLTYPE *yylocp, mc_value* mcast)
{
  const yybool yynormal YY_ATTRIBUTE_UNUSED = yystackp->yysplitPoint == YY_NULLPTR;
  int yylow = 1;
  YY_USE (yyvalp);
  YY_USE (yylocp);
  YY_USE (mcast);
  YY_USE (yyk);
  YY_USE (yyrhslen);
# undef yyerrok
# define yyerrok (yystackp->yyerrState = 0)
# undef YYACCEPT
# define YYACCEPT return yyaccept
# undef YYABORT
# define YYABORT return yyabort
# undef YYNOMEM
# define YYNOMEM return yynomem
# undef YYERROR
# define YYERROR return yyerrok, yyerr
# undef YYRECOVERING
# define YYRECOVERING() (yystackp->yyerrState != 0)
# undef yyclearin
# define yyclearin (yychar = MCA_EMPTY)
# undef YYFILL
# define YYFILL(N) yyfill (yyvsp, &yylow, (N), yynormal)
# undef YYBACKUP
# define YYBACKUP(Token, Value)                                              \
  return yyerror (yylocp, mcast, YY_("syntax error: cannot back up")),     \
         yyerrok, yyerr
  if (yyrhslen == 0)
    *yyvalp = yyval_default;
  else
    *yyvalp = yyvsp[YYFILL (1-yyrhslen)].yystate.yysemantics.yyval;
  /* Default location. */
  YYLLOC_DEFAULT ((*yylocp), (yyvsp - yyrhslen), yyrhslen);
  yystackp->yyerror_range[1].yystate.yyloc = *yylocp;
  /* If yyk == -1, we are running a deferred action on a temporary
     stack.  In that case, YY_REDUCE_PRINT must not play with YYFILL,
     so pretend the stack is "normal". */
  YY_REDUCE_PRINT ((yynormal || yyk == -1, yyvsp, yyk, yyrule, mcast));
  switch (yyrule)
    {
  case 2:
               {}
    break;
  case 3:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 4:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 5:
                                 { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 6:
                        { ((*yyvalp).value) = NULL; }
    break;
  case 7:
                                 { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value); }
    break;
  case 8:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 9:
               { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 10:
                     { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 11:
                  { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 12:
                     { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 13:
                { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 14:
               { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 15:
                  { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 16:
              { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1002_TOP_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
    break;
  case 17:
{
    
    ((*yyvalp).value) = ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->next) ? mc_value_create_node(MCAST_USE_PUB, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
    break;
  case 18:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->next) {
        // Save mc_uri pos/len before mc_value_link extends it with IMPORT_IDS
        unsigned int uri_pos = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos;
        unsigned int uri_len = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len;
        ((*yyvalp).value) = mc_value_create_node(MCAST_USE_PUB, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_URI_IMPORT_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos = uri_pos;
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len = uri_len;
    } else {
        ((*yyvalp).value) = NULL;
    }
}
    break;
  case 19:
{
    
    ((*yyvalp).value) = ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->next) ? mc_value_create_node(MCAST_USE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
    break;
  case 20:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->next) {
        // Save mc_uri pos/len before mc_value_link extends it with IMPORT_IDS
        unsigned int uri_pos = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos;
        unsigned int uri_len = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len;
        ((*yyvalp).value) = mc_value_create_node(MCAST_USE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_URI_IMPORT_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos = uri_pos;
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len = uri_len;
    } else {
        ((*yyvalp).value) = NULL;
    }
}
    break;
  case 21:
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 22:
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 23:
{
    /* $1 == NULL when mc_uri_trunk recovered through `| mc_levels error`
       (bad char mid-path; E2092 already logged) — drop the whole uri. */
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) ? mc_value_link(mc_value_create_data(MCAST_URI_PREFIX, strdup("$"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
    break;
  case 24:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) ? mc_value_link3(
        mc_value_create_data(MCAST_URI_PREFIX, strdup("$"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
        mc_value_create_node(MCAST_URI_ASID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
    ) : NULL;
}
    break;
  case 25:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) ? mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
    break;
  case 26:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) ? mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_URI_ASID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            ) : NULL;
}
    break;
  case 27:
                                { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("/"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 28:
                                { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("./"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen);}
    break;
  case 29:
                                         { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("../"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 30:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 31:
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 32:
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 33:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
    break;
  case 34:
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
    break;
  case 35:
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
    break;
  case 36:
{
    
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 37:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 38:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 39:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 40:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 41:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 42:
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 43:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 44:
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);}
    break;
  case 45:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COMPONENT, mc_value_link(
            mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 46:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COMPONENT, mc_value_link(
            mc_value_create_node(MCAST_ABSTRACT, NULL),
            mc_value_link(
                mc_value_link3(
                    mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 47:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_MODULE, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 48:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INTERFACE, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 49:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ENUM, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)), 
                mc_value_create_node(MCAST_ENUM_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))
            ));
}
    break;
  case 50:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_BOM, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 51:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_RECIPE, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 52:
                   { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 53:
                    { ((*yyvalp).value) = mc_value_create_node(MCAST_VARIANT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 54:
                                               { ((*yyvalp).value) = NULL; }
    break;
  case 55:
                    { ((*yyvalp).value) = mc_value_create_node(MCAST_ADOPTS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 56:
                                               { ((*yyvalp).value) = NULL; }
    break;
  case 57:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1105_EMPTY_BODY, 2,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_BODY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 58:
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 59:
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 60:
                                            { ((*yyvalp).value) = NULL; }
    break;
  case 61:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 62:
                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 63:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 64:
                   { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 65:
                       { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 66:
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 67:
                      { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 68:
                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 69:
                   { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 70:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 71:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 72:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_LET, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 73:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_REQUIRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 74:
                 { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1003_CLAUSE_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
    break;
  case 75:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 76:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link(
                            mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                            mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 77:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_ADD, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 78:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)->sub,
                            mc_value_create_node(MCAST_OPD_DOT,
                                mc_value_create_data(MCAST_IDA, strdup((char*)(YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->data), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 79:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 80:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 81:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
    break;
  case 82:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
    break;
  case 83:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); 
}
    break;
  case 84:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 85:
                        {
    /* U372 leg4 (L13 face law): the empty single-quoted string survives on
     * the VALUE face only (`= ''`). The lexer now lexes adjacent apostrophes
     * as two transpose suffixes, so the operand face reads the same pair as
     * the involution `X''` (chained mc_phrase MCOP_APOST) — the face, not
     * the lexer, decides. Hung off mc_attr_value (not mc_literal) so the
     * APOST-start items stay out of the mc_expr/judge state graph. */
    
    ((*yyvalp).value) = mc_value_create_data(MCAST_STRING, strdup(""), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen);
}
    break;
  case 86:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_ATT_VALUES,
                    mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))))));
}
    break;
  case 87:
                                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 88:
                                                               { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 89:
                                                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 90:
                                                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 91:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 92:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PINADD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 93:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 94:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 95:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 96:
                                                    { ((*yyvalp).value) = NULL; }
    break;
  case 97:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 98:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 99:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 100:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 101:
{
    
    // power-intent trailing `@attr…` run (intent-design.md §5): the
    // tattr chain sits as extra sibling children after the name side, the same
    // shape conduit/domain/net rows carry — the Rust McAttribute reader
    // (collect on MCAST_ATTRIBUTE children) parses them unchanged.
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 102:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 103:
{
    
    // trailing `@attr…` run plus a `, label…` value tail (golden hs analog
    // supply row: `psnk [13,12] = [VDDA,VSSA]::DC(5V) @class(analog), "..."`).
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 104:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_link4(
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value),
                    mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 105:
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1004_PIN_SKIPPED, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 106:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 107:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 108:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 109:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 110:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 111:
{
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 112:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME,
            mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 113:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 114:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 115:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 116:
                                         { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 117:
                                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 118:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 119:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 120:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 121:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 4 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 6 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 7 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 9)) {
        mc_dlog_add(MCD_E1007_NET_NOT_PORT, 1,
                    g_last_token->tpos, g_last_token->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 122:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link(
            mc_value_create_data(MCAST_IOTYPE_RETURN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 123:
                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 124:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 125:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 126:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 127:
                                                                 { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 128:
                                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 131:
          {
              
              ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
          }
    break;
  case 132:
                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 133:
                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 134:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 135:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 136:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, 
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 137:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 138:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 139:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 140:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD,
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 141:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 142:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 143:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 144:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 145:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 146:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 147:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 148:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 149:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 150:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 151:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 152:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 153:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 154:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 155:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 156:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 157:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 158:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 159:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 160:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SPLICE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 161:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 162:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 163:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 164:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1103_TRANSPOSE_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 165:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 166:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1104_CARET_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 167:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_REORDER, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)));
}
    break;
  case 168:
{
    
    /* mc_opd-headed twin of the arm above, same shape as the `phr{n|m}`
       pair: after a bare operand the FIRST `{` is force-shifted at the
       mc_opd state (the state 602 shift into the pipe-arm continuation),
       so a phrase-headed arm alone never sees `X{{` — the operand has not
       reduced to mc_phrase on a LCURLY lookahead (default-reduce arm).
       The twin rides the same shift and dies/merges exactly where the
       phrase-headed arm does. */
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_REORDER, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)));
}
    break;
  case 169:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 170:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 171:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 172:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 173:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 174:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 175:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 176:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 177:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 178:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 179:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 180:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 181:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 182:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 183:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 184:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 185:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 186:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 187:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 188:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 189:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 190:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 191:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 192:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 193:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 194:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 195:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 196:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 197:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 198:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 199:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 200:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 201:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 202:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 203:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 204:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 205:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 206:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 207:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 208:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 209:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 210:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 211:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 212:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 213:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 214:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 215:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 216:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 217:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 218:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 219:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 220:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 221:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 222:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 223:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 224:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 225:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 226:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 227:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 228:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 229:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 230:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 231:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 232:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 233:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 234:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 235:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 236:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 237:
{
    
    // Explicit `this{n|m}` two-side pin selector. The generic `mc_opd { n | m }`
    // rule already accepts `this` (it is an mc_opd), but `%left MCK_THIS` sits
    // BELOW `MCPT_LCURLY` in precedence, so at `this . {` the parser is forced
    // to SHIFT `{` into `mc_idm → { mc_idans }` (the single-side `this{1,3}`
    // path) and the REDUCE to mc_opd — the only way to reach curlyMN — is
    // pruned; then `this{1|2}` / `this{\+|\-}` die at the `|`. Making curlyMN
    // reachable via a direct SHIFT (`MCK_THIS MCPT_LCURLY …`) keeps BOTH the
    // comma (mc_idm → OPD_CURLY) and the pipe (curlyMN → OPD_CURLY_MN)
    // interpretations alive under GLR until the separator decides.
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3(
            mc_value_create_node(MCAST_OPD,
                mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 238:
{
    
    // U347: `pins{n|m}` mirrors `this{n|m}` one for one — inside a container
    // body both keywords name the same self face (see the mc_opd: MCK_PINS
    // rules), and `%left MCK_PINS` sits in the same precedence group as
    // `%left MCK_THIS`, so `pins{a|b}` died at the `|` exactly the way
    // `this{1|2}` did before the arm above: the `{` was force-shifted into
    // `mc_idm → { mc_idans }` (comma-only) and the pipe pruned the stack.
    // The same direct-SHIFT arm keeps the comma and pipe interpretations
    // alive under GLR until the separator decides. The root keeps its own
    // MCAST_OPD_PINS payload — the semantic layer reads the self face off
    // the keyword the author spelled, not off a folded `this`.
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3(
            mc_value_create_node(MCAST_OPD,
                mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 239:
{
    
    // Wrap each mc_opds in an MCAST_OPDS node: mc_value_link3 flattens the
    // whole next-chain, so without the wrapper the `|` boundary between the
    // left/right option lists is lost (left side absorbs the right side's ids).
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 240:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 241:
{
    
    /* U372 leg5 (L14 brace ≡ list law, ruling ⑦): the bare comma form
       `{a, b}` opens as a member vector — the SAME node the bracket list
       produces (MCAST_OPD_SQUARE_VEC), so every downstream reader (Pass1
       shapes, Pass2 lanes, suffix operators, the value face) sees one
       vector literal with no new node to teach. The member grammar is the
       pipe arm's mc_mn_opds (not mc_phrases): the shared prefix keeps the
       two arms apart at `}` vs `|` with zero state merges, while
       mc_phrases would collide with mc_mn_opd's own `[...]` alternative
       inside the curly context (bison R/R family, +4 states). The pipe
       twin arm below keeps `{a | b}` (and `{a,b | c,d}`) on OPD_CURLY_MN:
       the face inside the braces — comma vs pipe — decides, the same way
       the lexer hands `''` to the parser and the face decides (leg4). The
       postfix member-select `X{a,b}` (mc_idm → OPD_CURLY) is untouched:
       that form is a selection on a base, this one is a base-less
       literal. */
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 242:
{
    
    /* U339: bare two-face row `{a | b}` (no base operand). After `return`
       the old arrangement died whole-file: `return` can reduce to a name
       (mc_ida_run_atom), whose IDA_BASE_PREC sits below MCPT_LCURLY, so the
       `{` was force-shifted into `mc_idm -> { mc_idans }` (comma-only) and
       the `|` killed the stack — the body-block arm never survived either.
       A direct-SHIFT phrase arm keeps the base-less row alive the same way
       the `this{n|m}` keyword arm above keeps its form alive. Structure
       marks the base-less shape: the first child is the left MCAST_OPDS
       wrapper itself (a `X{n|m}` base is a single MCAST_OPD), so the
       reader can tell the two apart and read the sides as verbatim row
       endpoints (a row: left face = a, right face = b). */
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link(
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 243:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 244:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 245:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 246:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 247:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 248:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 249:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 250:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 251:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 252:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 253:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 254:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 255:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 256:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 257:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 258:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 259:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 260:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 261:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 262:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 263:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 264:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 265:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 266:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 267:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 268:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 269:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 270:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ROLE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 271:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_REF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));   // MCAST_REF kept as internal node name
}
    break;
  case 272:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DOMAIN, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 273:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_RAIL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 274:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARTITION, mc_value_link3(
                    mc_value_create_data(MCAST_PARTITION_LEVEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 275:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARTITION, mc_value_link3(
                    mc_value_create_data(MCAST_PARTITION_LEVEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen),
                    mc_value_create_node(MCAST_IDS, NULL),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 276:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ERROR, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 277:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 278:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 279:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 280:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 281:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 282:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 283:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 284:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 285:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 286:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 287:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 288:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 289:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES,
                mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 290:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 291:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 292:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 293:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 294:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 295:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 304:
{
    char buf[160];
    snprintf(buf, sizeof buf,
             "Reserved word '%s' cannot be an attribute value (reserved words "
             "are not names anywhere); pick another spelling.", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring);
    mc_dlog_add(MCD_E1032_TATTR_RESWORD, 1, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, buf);
    ((*yyvalp).value) = NULL;
}
    break;
  case 305:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_FUNCTION, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 306:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 307:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 308:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 309:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 310:
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 311:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 312:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link(
        mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 313:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 314:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 315:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 316:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 317:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 318:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 319:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 320:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 321:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value))))
        );
}
    break;
  case 322:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))
        );
}
    break;
  case 323:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, mc_value_link(
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), 
                    mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))
            )
        );
}
    break;
  case 324:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 325:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 326:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 327: /* mc_declare_a1: mc_ids mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 328: /* mc_declare_a1: mc_ids MCPT_DOT mc_int mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 329: /* mc_declare_a1: mc_ids MCPT_LPAREN mc_params MCPT_RPAREN mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 330:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 331:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 332:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 333:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 335:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 336:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value))));
}
    break;
  case 337:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                        mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                        mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-8)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value))))
                    ));
}
    break;
  case 338:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 339:
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 340:
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 341:
                                            { ((*yyvalp).value) = NULL; }
    break;
  case 342:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 343:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 344:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 345:
                                                    { ((*yyvalp).value) = NULL; }
    break;
  case 346:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 347:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 348:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 349:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 350:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 351:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 352:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES,
            mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))));
}
    break;
  case 353:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 354:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES,
            mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))));
}
    break;
  case 355:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
            mc_value_link(
                mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 356:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_IF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 357:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), 
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 358:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 359:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link4(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 360:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 361:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 362:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 363:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 364:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 365:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 366:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 367:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 368:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_EQEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 369:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_AND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 370:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_OR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 371:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_NOTEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 372:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));}
    break;
  case 373:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATERTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 374:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSEQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 375:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATEREQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 376:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITAND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 377:
                                                { mc_dlog_add(MCD_W1101_SINGLE_OR, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITOR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 378:
                                                { mc_dlog_add(MCD_W1102_PLUSMINUS, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 379:
                                                               { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_IN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))); }
    break;
  case 380:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 381:
          { 
            mc_dlog_add(MCD_E1017_JUDGE_ERROR, 1, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len, NULL);
            ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BARE_LITERAL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 382:
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 383:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 384:
        {
            unsigned int start = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos;
            unsigned int end = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len;
            ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strndup(g_lex_src + start, end - start), start, end - start);
        }
    break;
  case 385:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 386:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 387:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 388:
        {
            // Transient span carrier for the chain; the data field is unused.
            ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strdup(""), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen) - (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos);
        }
    break;
  case 389:
        {
            // U385 leg G1 (b4500): the per-layer explicit-expansion marker
            // `name[[range|points]]` (nets.mc:449-457, layer-expansion-law.md
            // §3) — any bracket of the tail chain may carry the doubled
            // spelling (`S[[1:4]][L,R]` marks the outer axis, `S[1:4][[L,R]]`
            // the inner). Same transient span carrier as the plain arm: the
            // `[[ ]]` spelling survives inside the byte-exact IDA string and
            // mcc's McIda scanner owns the mark, so no new AST node joins the
            // family (astdef unchanged). `[[*]]` stays shut — `*` is not an
            // mc_ida_run_atom, so the §15.1 wildcard ban covers the new
            // spelling without a dedicated arm.
            ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strdup(""), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.token)->tpos, ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen) - (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.token)->tpos);
        }
    break;
  case 390:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 391:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 392:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 393:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 394:
                                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 395:
                                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 396:
                                                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 397:
                                                         { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_REORDER_RANGE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 398:
        { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 500:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 501:
                                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 502:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 503:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 504:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 505:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 506:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 507:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 508:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 509:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 510:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 511:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 512:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 513:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 514:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 515:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 516:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 517:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 518:
                            { ((*yyvalp).value) = mc_value_create_data(MCAST_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 519:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 520:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 521:
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 522:
                       { ((*yyvalp).value) = mc_value_create_data(MCAST_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 523:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 524:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 525:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 526:
                               { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_USCORE, strdup("_"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 527:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 528:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 529:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 530:
                        {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 531:
                                                                     {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 532:
                                                                    {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 533:
                                                                       {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
                        mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "~", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
        }
    break;
  case 534:
                                                                                    {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
                        mc_value_create_data(MCAST_ID, mc_cond_call_text((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen - (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)->pos)));
        }
    break;
  case 535:
                                            {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 536:
                                       {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 537:
                                   {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 538:
                                   {
            
            ((*yyvalp).value) = mc_value_create_node_spanned(MCAST_UVALUE, mc_value_create_data(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->type, mc_sign_joined((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value), '-'), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len);
        }
    break;
  case 539:
                                  {
            
            ((*yyvalp).value) = mc_value_create_node_spanned(MCAST_UVALUE, mc_value_create_data(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->type, mc_sign_joined((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value), '+'), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len);
        }
    break;
  case 540:
                               {
            
            ((*yyvalp).value) = mc_value_create_data((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->type, mc_sign_joined((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value), '-'),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len);
        }
    break;
  case 541:
                              {
            
            ((*yyvalp).value) = mc_value_create_data((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->type, mc_sign_joined((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value), '+'),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len);
        }
    break;
  case 542:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 543:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "-", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 544:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "-", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 545:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "+", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 546:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "<", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 547:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), ">", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 548:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "<=", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 549:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), ">=", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 550:
{
    ((*yyvalp).value) = mc_cond_join_op(mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), "", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 551:
{
    ((*yyvalp).value) = mc_cond_join_op(mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), "", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 552:
{
    ((*yyvalp).value) = mc_cond_join_op(mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), "", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 553:
{
    ((*yyvalp).value) = mc_cond_join_op(mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), "", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 554:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 555:
{
    ((*yyvalp).value) = mc_cond_join_op((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), "-", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 556:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 557:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_OUT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 558:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IO, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 559:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSRC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 560:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSNK, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 561:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSBI, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 562:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 563:
                     { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_RF, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 566:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 567:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 568:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 569:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 570:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 571:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 572:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 573:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 574:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 575:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 576:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 577:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 578:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 579:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 580:
                  { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 581:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 582:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 583:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 584:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 585:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 586:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 587:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 588:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 589:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 590:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 591:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 592:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 593:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 594:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_COMPOSITE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 595:
        { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 596:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 597:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 598:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 599:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 600:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 601:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 602:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 603:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 604:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 605:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 606:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 607:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 608:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 609:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 610:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 611:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 612:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 613:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 614:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 615:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 616:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 617:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 618:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 619:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 620:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 621:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 622:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 623:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 624:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 625:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 626:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 627:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_DIV, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 628:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_MUL, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 629:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
	}
    break;
  case 630:
                  {}
    break;
  case 631:
                         {}
    break;
  case 632:
                           {}
    break;
  case 633:
                                  {}
    break;
      default: break;
    }
  YY_SYMBOL_PRINT ("-> $$ =", yylhsNonterm (yyrule), yyvalp, yylocp);
  return yyok;
# undef yyerrok
# undef YYABORT
# undef YYACCEPT
# undef YYNOMEM
# undef YYERROR
# undef YYBACKUP
# undef yyclearin
# undef YYRECOVERING
}
static void
yyuserMerge (int yyn, YYSTYPE* yy0, YYSTYPE* yy1)
{
  YY_USE (yy0);
  YY_USE (yy1);
  switch (yyn)
    {
      default: break;
    }
}
                              /* Bison grammar-table manipulation.  */
/*-----------------------------------------------.
| Release the memory associated to this symbol.  |
`-----------------------------------------------*/
static void
yydestruct (const char *yymsg,
            yysymbol_kind_t yykind, YYSTYPE *yyvaluep, YYLTYPE *yylocationp, mc_value* mcast)
{
  YY_USE (yyvaluep);
  YY_USE (yylocationp);
  YY_USE (mcast);
  if (!yymsg)
    yymsg = "Deleting";
  YY_SYMBOL_PRINT (yymsg, yykind, yyvaluep, yylocationp);
  YY_IGNORE_MAYBE_UNINITIALIZED_BEGIN
  YY_USE (yykind);
  YY_IGNORE_MAYBE_UNINITIALIZED_END
}
/** Number of symbols composing the right hand side of rule #RULE.  */
static inline int
yyrhsLength (yyRuleNum yyrule)
{
  return yyr2[yyrule];
}
static void
yydestroyGLRState (char const *yymsg, yyGLRState *yys, mc_value* mcast)
{
  if (yys->yyresolved)
    yydestruct (yymsg, yy_accessing_symbol (yys->yylrState),
                &yys->yysemantics.yyval, &yys->yyloc, mcast);
  else
    {
#if MCA_DEBUG
      if (yydebug)
        {
          if (yys->yysemantics.yyfirstVal)
            YY_FPRINTF ((stderr, "%s unresolved", yymsg));
          else
            YY_FPRINTF ((stderr, "%s incomplete", yymsg));
          YY_SYMBOL_PRINT ("", yy_accessing_symbol (yys->yylrState), YY_NULLPTR, &yys->yyloc);
        }
#endif
      if (yys->yysemantics.yyfirstVal)
        {
          yySemanticOption *yyoption = yys->yysemantics.yyfirstVal;
          yyGLRState *yyrh;
          int yyn;
          for (yyrh = yyoption->yystate, yyn = yyrhsLength (yyoption->yyrule);
               yyn > 0;
               yyrh = yyrh->yypred, yyn -= 1)
            yydestroyGLRState (yymsg, yyrh, mcast);
        }
    }
}
#define yypact_value_is_default(Yyn) \
  ((Yyn) == YYPACT_NINF)
/** True iff LR state YYSTATE has only a default reduction (regardless
 *  of token).  */
static inline yybool
yyisDefaultedState (yy_state_t yystate)
{
  return yypact_value_is_default (yypact[yystate]);
}
/** The default reduction for YYSTATE, assuming it has one.  */
static inline yyRuleNum
yydefaultAction (yy_state_t yystate)
{
  return yydefact[yystate];
}
#define yytable_value_is_error(Yyn) \
  0
/** The action to take in YYSTATE on seeing YYTOKEN.
 *  Result R means
 *    R < 0:  Reduce on rule -R.
 *    R = 0:  Error.
 *    R > 0:  Shift to state R.
 *  Set *YYCONFLICTS to a pointer into yyconfl to a 0-terminated list
 *  of conflicting reductions.
 */
static inline int
yygetLRActions (yy_state_t yystate, yysymbol_kind_t yytoken, const short** yyconflicts)
{
  int yyindex = yypact[yystate] + yytoken;
  if (yytoken == YYSYMBOL_YYerror)
    {
      // This is the error token.
      *yyconflicts = yyconfl;
      return 0;
    }
  else if (yyisDefaultedState (yystate)
           || yyindex < 0 || YYLAST < yyindex || yycheck[yyindex] != yytoken)
    {
      *yyconflicts = yyconfl;
      return -yydefact[yystate];
    }
  else if (! yytable_value_is_error (yytable[yyindex]))
    {
      *yyconflicts = yyconfl + yyconflp[yyindex];
      return yytable[yyindex];
    }
  else
    {
      *yyconflicts = yyconfl + yyconflp[yyindex];
      return 0;
    }
}
/** Compute post-reduction state.
 * \param yystate   the current state
 * \param yysym     the nonterminal to push on the stack
 */
static inline yy_state_t
yyLRgotoState (yy_state_t yystate, yysymbol_kind_t yysym)
{
  int yyr = yypgoto[yysym - YYNTOKENS] + yystate;
  if (0 <= yyr && yyr <= YYLAST && yycheck[yyr] == yystate)
    return yytable[yyr];
  else
    return yydefgoto[yysym - YYNTOKENS];
}
static inline yybool
yyisShiftAction (int yyaction)
{
  return 0 < yyaction;
}
static inline yybool
yyisErrorAction (int yyaction)
{
  return yyaction == 0;
}
                                /* GLRStates */
/** Return a fresh GLRStackItem in YYSTACKP.  The item is an LR state
 *  if YYISSTATE, and otherwise a semantic option.  Callers should call
 *  YY_RESERVE_GLRSTACK afterwards to make sure there is sufficient
 *  headroom.  */
static inline yyGLRStackItem*
yynewGLRStackItem (yyGLRStack* yystackp, yybool yyisState)
{
  yyGLRStackItem* yynewItem = yystackp->yynextFree;
  yystackp->yyspaceLeft -= 1;
  yystackp->yynextFree += 1;
  yynewItem->yystate.yyisState = yyisState;
  return yynewItem;
}
/** Add a new semantic action that will execute the action for rule
 *  YYRULE on the semantic values in YYRHS to the list of
 *  alternative actions for YYSTATE.  Assumes that YYRHS comes from
 *  stack #YYK of *YYSTACKP. */
static void
yyaddDeferredAction (yyGLRStack* yystackp, YYPTRDIFF_T yyk, yyGLRState* yystate,
                     yyGLRState* yyrhs, yyRuleNum yyrule)
{
  yySemanticOption* yynewOption =
    &yynewGLRStackItem (yystackp, yyfalse)->yyoption;
  YY_ASSERT (!yynewOption->yyisState);
  yynewOption->yystate = yyrhs;
  yynewOption->yyrule = yyrule;
  if (yystackp->yytops.yylookaheadNeeds[yyk])
    {
      yynewOption->yyrawchar = yychar;
      yynewOption->yyval = yylval;
      yynewOption->yyloc = yylloc;
    }
  else
    yynewOption->yyrawchar = MCA_EMPTY;
  yynewOption->yynext = yystate->yysemantics.yyfirstVal;
  yystate->yysemantics.yyfirstVal = yynewOption;
  YY_RESERVE_GLRSTACK (yystackp);
}
                                /* GLRStacks */
/** Initialize YYSET to a singleton set containing an empty stack.  */
static yybool
yyinitStateSet (yyGLRStateSet* yyset)
{
  yyset->yysize = 1;
  yyset->yycapacity = 16;
  yyset->yystates
    = YY_CAST (yyGLRState**,
               YYMALLOC (YY_CAST (YYSIZE_T, yyset->yycapacity)
                         * sizeof yyset->yystates[0]));
  if (! yyset->yystates)
    return yyfalse;
  yyset->yystates[0] = YY_NULLPTR;
  yyset->yylookaheadNeeds
    = YY_CAST (yybool*,
               YYMALLOC (YY_CAST (YYSIZE_T, yyset->yycapacity)
                         * sizeof yyset->yylookaheadNeeds[0]));
  if (! yyset->yylookaheadNeeds)
    {
      YYFREE (yyset->yystates);
      return yyfalse;
    }
  memset (yyset->yylookaheadNeeds,
          0,
          YY_CAST (YYSIZE_T, yyset->yycapacity) * sizeof yyset->yylookaheadNeeds[0]);
  return yytrue;
}
static void yyfreeStateSet (yyGLRStateSet* yyset)
{
  YYFREE (yyset->yystates);
  YYFREE (yyset->yylookaheadNeeds);
}
/** Initialize *YYSTACKP to a single empty stack, with total maximum
 *  capacity for all stacks of YYSIZE.  */
static yybool
yyinitGLRStack (yyGLRStack* yystackp, YYPTRDIFF_T yysize)
{
  yystackp->yyerrState = 0;
  yynerrs = 0;
  yystackp->yyspaceLeft = yysize;
  yystackp->yyitems
    = YY_CAST (yyGLRStackItem*,
               YYMALLOC (YY_CAST (YYSIZE_T, yysize)
                         * sizeof yystackp->yynextFree[0]));
  if (!yystackp->yyitems)
    return yyfalse;
  yystackp->yynextFree = yystackp->yyitems;
  yystackp->yysplitPoint = YY_NULLPTR;
  yystackp->yylastDeleted = YY_NULLPTR;
  return yyinitStateSet (&yystackp->yytops);
}
#if YYSTACKEXPANDABLE
# define YYRELOC(YYFROMITEMS, YYTOITEMS, YYX, YYTYPE)                   \
  &((YYTOITEMS)                                                         \
    - ((YYFROMITEMS) - YY_REINTERPRET_CAST (yyGLRStackItem*, (YYX))))->YYTYPE
/** If *YYSTACKP is expandable, extend it.  WARNING: Pointers into the
    stack from outside should be considered invalid after this call.
    We always expand when there are 1 or fewer items left AFTER an
    allocation, so that we can avoid having external pointers exist
    across an allocation.  */
static void
yyexpandGLRStack (yyGLRStack* yystackp)
{
  yyGLRStackItem* yynewItems;
  yyGLRStackItem* yyp0, *yyp1;
  YYPTRDIFF_T yynewSize;
  YYPTRDIFF_T yyn;
  YYPTRDIFF_T yysize = yystackp->yynextFree - yystackp->yyitems;
  if (YYMAXDEPTH - YYHEADROOM < yysize)
    yyMemoryExhausted (yystackp);
  yynewSize = 2*yysize;
  if (YYMAXDEPTH < yynewSize)
    yynewSize = YYMAXDEPTH;
  yynewItems
    = YY_CAST (yyGLRStackItem*,
               YYMALLOC (YY_CAST (YYSIZE_T, yynewSize)
                         * sizeof yynewItems[0]));
  if (! yynewItems)
    yyMemoryExhausted (yystackp);
  for (yyp0 = yystackp->yyitems, yyp1 = yynewItems, yyn = yysize;
       0 < yyn;
       yyn -= 1, yyp0 += 1, yyp1 += 1)
    {
      *yyp1 = *yyp0;
      if (*YY_REINTERPRET_CAST (yybool *, yyp0))
        {
          yyGLRState* yys0 = &yyp0->yystate;
          yyGLRState* yys1 = &yyp1->yystate;
          if (yys0->yypred != YY_NULLPTR)
            yys1->yypred =
              YYRELOC (yyp0, yyp1, yys0->yypred, yystate);
          if (! yys0->yyresolved && yys0->yysemantics.yyfirstVal != YY_NULLPTR)
            yys1->yysemantics.yyfirstVal =
              YYRELOC (yyp0, yyp1, yys0->yysemantics.yyfirstVal, yyoption);
        }
      else
        {
          yySemanticOption* yyv0 = &yyp0->yyoption;
          yySemanticOption* yyv1 = &yyp1->yyoption;
          if (yyv0->yystate != YY_NULLPTR)
            yyv1->yystate = YYRELOC (yyp0, yyp1, yyv0->yystate, yystate);
          if (yyv0->yynext != YY_NULLPTR)
            yyv1->yynext = YYRELOC (yyp0, yyp1, yyv0->yynext, yyoption);
        }
    }
  if (yystackp->yysplitPoint != YY_NULLPTR)
    yystackp->yysplitPoint = YYRELOC (yystackp->yyitems, yynewItems,
                                      yystackp->yysplitPoint, yystate);
  for (yyn = 0; yyn < yystackp->yytops.yysize; yyn += 1)
    if (yystackp->yytops.yystates[yyn] != YY_NULLPTR)
      yystackp->yytops.yystates[yyn] =
        YYRELOC (yystackp->yyitems, yynewItems,
                 yystackp->yytops.yystates[yyn], yystate);
  YYFREE (yystackp->yyitems);
  yystackp->yyitems = yynewItems;
  yystackp->yynextFree = yynewItems + yysize;
  yystackp->yyspaceLeft = yynewSize - yysize;
}
#endif
static void
yyfreeGLRStack (yyGLRStack* yystackp)
{
  YYFREE (yystackp->yyitems);
  yyfreeStateSet (&yystackp->yytops);
}
/** Assuming that YYS is a GLRState somewhere on *YYSTACKP, update the
 *  splitpoint of *YYSTACKP, if needed, so that it is at least as deep as
 *  YYS.  */
static inline void
yyupdateSplit (yyGLRStack* yystackp, yyGLRState* yys)
{
  if (yystackp->yysplitPoint != YY_NULLPTR && yystackp->yysplitPoint > yys)
    yystackp->yysplitPoint = yys;
}
/** Invalidate stack #YYK in *YYSTACKP.  */
static inline void
yymarkStackDeleted (yyGLRStack* yystackp, YYPTRDIFF_T yyk)
{
  if (yystackp->yytops.yystates[yyk] != YY_NULLPTR)
    yystackp->yylastDeleted = yystackp->yytops.yystates[yyk];
  yystackp->yytops.yystates[yyk] = YY_NULLPTR;
}
/** Undelete the last stack in *YYSTACKP that was marked as deleted.  Can
    only be done once after a deletion, and only when all other stacks have
    been deleted.  */
static void
yyundeleteLastStack (yyGLRStack* yystackp)
{
  if (yystackp->yylastDeleted == YY_NULLPTR || yystackp->yytops.yysize != 0)
    return;
  yystackp->yytops.yystates[0] = yystackp->yylastDeleted;
  yystackp->yytops.yysize = 1;
  YY_DPRINTF ((stderr, "Restoring last deleted stack as stack #0.\n"));
  yystackp->yylastDeleted = YY_NULLPTR;
}
static inline void
yyremoveDeletes (yyGLRStack* yystackp)
{
  YYPTRDIFF_T yyi, yyj;
  yyi = yyj = 0;
  while (yyj < yystackp->yytops.yysize)
    {
      if (yystackp->yytops.yystates[yyi] == YY_NULLPTR)
        {
          if (yyi == yyj)
            YY_DPRINTF ((stderr, "Removing dead stacks.\n"));
          yystackp->yytops.yysize -= 1;
        }
      else
        {
          yystackp->yytops.yystates[yyj] = yystackp->yytops.yystates[yyi];
          /* In the current implementation, it's unnecessary to copy
             yystackp->yytops.yylookaheadNeeds[yyi] since, after
             yyremoveDeletes returns, the parser immediately either enters
             deterministic operation or shifts a token.  However, it doesn't
             hurt, and the code might evolve to need it.  */
          yystackp->yytops.yylookaheadNeeds[yyj] =
            yystackp->yytops.yylookaheadNeeds[yyi];
          if (yyj != yyi)
            YY_DPRINTF ((stderr, "Rename stack %ld -> %ld.\n",
                        YY_CAST (long, yyi), YY_CAST (long, yyj)));
          yyj += 1;
        }
      yyi += 1;
    }
}
/** Shift to a new state on stack #YYK of *YYSTACKP, corresponding to LR
 * state YYLRSTATE, at input position YYPOSN, with (resolved) semantic
 * value *YYVALP and source location *YYLOCP.  */
static inline void
yyglrShift (yyGLRStack* yystackp, YYPTRDIFF_T yyk, yy_state_t yylrState,
            YYPTRDIFF_T yyposn,
            YYSTYPE* yyvalp, YYLTYPE* yylocp)
{
  yyGLRState* yynewState = &yynewGLRStackItem (yystackp, yytrue)->yystate;
  yynewState->yylrState = yylrState;
  yynewState->yyposn = yyposn;
  yynewState->yyresolved = yytrue;
  yynewState->yypred = yystackp->yytops.yystates[yyk];
  yynewState->yysemantics.yyval = *yyvalp;
  yynewState->yyloc = *yylocp;
  yystackp->yytops.yystates[yyk] = yynewState;
  YY_RESERVE_GLRSTACK (yystackp);
}
/** Shift stack #YYK of *YYSTACKP, to a new state corresponding to LR
 *  state YYLRSTATE, at input position YYPOSN, with the (unresolved)
 *  semantic value of YYRHS under the action for YYRULE.  */
static inline void
yyglrShiftDefer (yyGLRStack* yystackp, YYPTRDIFF_T yyk, yy_state_t yylrState,
                 YYPTRDIFF_T yyposn, yyGLRState* yyrhs, yyRuleNum yyrule)
{
  yyGLRState* yynewState = &yynewGLRStackItem (yystackp, yytrue)->yystate;
  YY_ASSERT (yynewState->yyisState);
  yynewState->yylrState = yylrState;
  yynewState->yyposn = yyposn;
  yynewState->yyresolved = yyfalse;
  yynewState->yypred = yystackp->yytops.yystates[yyk];
  yynewState->yysemantics.yyfirstVal = YY_NULLPTR;
  yystackp->yytops.yystates[yyk] = yynewState;
  /* Invokes YY_RESERVE_GLRSTACK.  */
  yyaddDeferredAction (yystackp, yyk, yynewState, yyrhs, yyrule);
}
#if MCA_DEBUG
/*----------------------------------------------------------------------.
| Report that stack #YYK of *YYSTACKP is going to be reduced by YYRULE. |
`----------------------------------------------------------------------*/
static inline void
yy_reduce_print (yybool yynormal, yyGLRStackItem* yyvsp, YYPTRDIFF_T yyk,
                 yyRuleNum yyrule, mc_value* mcast)
{
  int yynrhs = yyrhsLength (yyrule);
  int yylow = 1;
  int yyi;
  YY_FPRINTF ((stderr, "Reducing stack %ld by rule %d (line %d):\n",
               YY_CAST (long, yyk), yyrule - 1, yyrline[yyrule]));
  if (! yynormal)
    yyfillin (yyvsp, 1, -yynrhs);
  /* The symbols being reduced.  */
  for (yyi = 0; yyi < yynrhs; yyi++)
    {
      YY_FPRINTF ((stderr, "   $%d = ", yyi + 1));
      yy_symbol_print (stderr,
                       yy_accessing_symbol (yyvsp[yyi - yynrhs + 1].yystate.yylrState),
                       &yyvsp[yyi - yynrhs + 1].yystate.yysemantics.yyval,
                       &(YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL ((yyi + 1) - (yynrhs))].yystate.yyloc)                       , mcast);
      if (!yyvsp[yyi - yynrhs + 1].yystate.yyresolved)
        YY_FPRINTF ((stderr, " (unresolved)"));
      YY_FPRINTF ((stderr, "\n"));
    }
}
#endif
/** Pop the symbols consumed by reduction #YYRULE from the top of stack
 *  #YYK of *YYSTACKP, and perform the appropriate semantic action on their
 *  semantic values.  Assumes that all ambiguities in semantic values
 *  have been previously resolved.  Set *YYVALP to the resulting value,
 *  and *YYLOCP to the computed location (if any).  Return value is as
 *  for userAction.  */
static inline YYRESULTTAG
yydoAction (yyGLRStack* yystackp, YYPTRDIFF_T yyk, yyRuleNum yyrule,
            YYSTYPE* yyvalp, YYLTYPE *yylocp, mc_value* mcast)
{
  int yynrhs = yyrhsLength (yyrule);
  if (yystackp->yysplitPoint == YY_NULLPTR)
    {
      /* Standard special case: single stack.  */
      yyGLRStackItem* yyrhs
        = YY_REINTERPRET_CAST (yyGLRStackItem*, yystackp->yytops.yystates[yyk]);
      YY_ASSERT (yyk == 0);
      yystackp->yynextFree -= yynrhs;
      yystackp->yyspaceLeft += yynrhs;
      yystackp->yytops.yystates[0] = & yystackp->yynextFree[-1].yystate;
      return yyuserAction (yyrule, yynrhs, yyrhs, yystackp, yyk,
                           yyvalp, yylocp, mcast);
    }
  else
    {
      yyGLRStackItem yyrhsVals[YYMAXRHS + YYMAXLEFT + 1];
      yyGLRState* yys = yyrhsVals[YYMAXRHS + YYMAXLEFT].yystate.yypred
        = yystackp->yytops.yystates[yyk];
      int yyi;
      if (yynrhs == 0)
        /* Set default location.  */
        yyrhsVals[YYMAXRHS + YYMAXLEFT - 1].yystate.yyloc = yys->yyloc;
      for (yyi = 0; yyi < yynrhs; yyi += 1)
        {
          yys = yys->yypred;
          YY_ASSERT (yys);
        }
      yyupdateSplit (yystackp, yys);
      yystackp->yytops.yystates[yyk] = yys;
      return yyuserAction (yyrule, yynrhs, yyrhsVals + YYMAXRHS + YYMAXLEFT - 1,
                           yystackp, yyk, yyvalp, yylocp, mcast);
    }
}
/** Pop items off stack #YYK of *YYSTACKP according to grammar rule YYRULE,
 *  and push back on the resulting nonterminal symbol.  Perform the
 *  semantic action associated with YYRULE and store its value with the
 *  newly pushed state, if YYFORCEEVAL or if *YYSTACKP is currently
 *  unambiguous.  Otherwise, store the deferred semantic action with
 *  the new state.  If the new state would have an identical input
 *  position, LR state, and predecessor to an existing state on the stack,
 *  it is identified with that existing state, eliminating stack #YYK from
 *  *YYSTACKP.  In this case, the semantic value is
 *  added to the options for the existing state's semantic value.
 */
static inline YYRESULTTAG
yyglrReduce (yyGLRStack* yystackp, YYPTRDIFF_T yyk, yyRuleNum yyrule,
             yybool yyforceEval, mc_value* mcast)
{
  YYPTRDIFF_T yyposn = yystackp->yytops.yystates[yyk]->yyposn;
  if (yyforceEval || yystackp->yysplitPoint == YY_NULLPTR)
    {
      YYSTYPE yyval;
      YYLTYPE yyloc;
      YYRESULTTAG yyflag = yydoAction (yystackp, yyk, yyrule, &yyval, &yyloc, mcast);
      if (yyflag == yyerr && yystackp->yysplitPoint != YY_NULLPTR)
        YY_DPRINTF ((stderr,
                     "Parse on stack %ld rejected by rule %d (line %d).\n",
                     YY_CAST (long, yyk), yyrule - 1, yyrline[yyrule]));
      if (yyflag != yyok)
        return yyflag;
      yyglrShift (yystackp, yyk,
                  yyLRgotoState (yystackp->yytops.yystates[yyk]->yylrState,
                                 yylhsNonterm (yyrule)),
                  yyposn, &yyval, &yyloc);
    }
  else
    {
      YYPTRDIFF_T yyi;
      int yyn;
      yyGLRState* yys, *yys0 = yystackp->yytops.yystates[yyk];
      yy_state_t yynewLRState;
      for (yys = yystackp->yytops.yystates[yyk], yyn = yyrhsLength (yyrule);
           0 < yyn; yyn -= 1)
        {
          yys = yys->yypred;
          YY_ASSERT (yys);
        }
      yyupdateSplit (yystackp, yys);
      yynewLRState = yyLRgotoState (yys->yylrState, yylhsNonterm (yyrule));
      YY_DPRINTF ((stderr,
                   "Reduced stack %ld by rule %d (line %d); action deferred.  "
                   "Now in state %d.\n",
                   YY_CAST (long, yyk), yyrule - 1, yyrline[yyrule],
                   yynewLRState));
      for (yyi = 0; yyi < yystackp->yytops.yysize; yyi += 1)
        if (yyi != yyk && yystackp->yytops.yystates[yyi] != YY_NULLPTR)
          {
            yyGLRState *yysplit = yystackp->yysplitPoint;
            yyGLRState *yyp = yystackp->yytops.yystates[yyi];
            while (yyp != yys && yyp != yysplit && yyp->yyposn >= yyposn)
              {
                if (yyp->yylrState == yynewLRState && yyp->yypred == yys)
                  {
                    yyaddDeferredAction (yystackp, yyk, yyp, yys0, yyrule);
                    yymarkStackDeleted (yystackp, yyk);
                    YY_DPRINTF ((stderr, "Merging stack %ld into stack %ld.\n",
                                 YY_CAST (long, yyk), YY_CAST (long, yyi)));
                    return yyok;
                  }
                yyp = yyp->yypred;
              }
          }
      yystackp->yytops.yystates[yyk] = yys;
      yyglrShiftDefer (yystackp, yyk, yynewLRState, yyposn, yys0, yyrule);
    }
  return yyok;
}
static YYPTRDIFF_T
yysplitStack (yyGLRStack* yystackp, YYPTRDIFF_T yyk)
{
  if (yystackp->yysplitPoint == YY_NULLPTR)
    {
      YY_ASSERT (yyk == 0);
      yystackp->yysplitPoint = yystackp->yytops.yystates[yyk];
    }
  if (yystackp->yytops.yycapacity <= yystackp->yytops.yysize)
    {
      YYPTRDIFF_T state_size = YYSIZEOF (yystackp->yytops.yystates[0]);
      YYPTRDIFF_T half_max_capacity = YYSIZE_MAXIMUM / 2 / state_size;
      if (half_max_capacity < yystackp->yytops.yycapacity)
        yyMemoryExhausted (yystackp);
      yystackp->yytops.yycapacity *= 2;
      {
        yyGLRState** yynewStates
          = YY_CAST (yyGLRState**,
                     YYREALLOC (yystackp->yytops.yystates,
                                (YY_CAST (YYSIZE_T, yystackp->yytops.yycapacity)
                                 * sizeof yynewStates[0])));
        if (yynewStates == YY_NULLPTR)
          yyMemoryExhausted (yystackp);
        yystackp->yytops.yystates = yynewStates;
      }
      {
        yybool* yynewLookaheadNeeds
          = YY_CAST (yybool*,
                     YYREALLOC (yystackp->yytops.yylookaheadNeeds,
                                (YY_CAST (YYSIZE_T, yystackp->yytops.yycapacity)
                                 * sizeof yynewLookaheadNeeds[0])));
        if (yynewLookaheadNeeds == YY_NULLPTR)
          yyMemoryExhausted (yystackp);
        yystackp->yytops.yylookaheadNeeds = yynewLookaheadNeeds;
      }
    }
  yystackp->yytops.yystates[yystackp->yytops.yysize]
    = yystackp->yytops.yystates[yyk];
  yystackp->yytops.yylookaheadNeeds[yystackp->yytops.yysize]
    = yystackp->yytops.yylookaheadNeeds[yyk];
  yystackp->yytops.yysize += 1;
  return yystackp->yytops.yysize - 1;
}
/** True iff YYY0 and YYY1 represent identical options at the top level.
 *  That is, they represent the same rule applied to RHS symbols
 *  that produce the same terminal symbols.  */
static yybool
yyidenticalOptions (yySemanticOption* yyy0, yySemanticOption* yyy1)
{
  if (yyy0->yyrule == yyy1->yyrule)
    {
      yyGLRState *yys0, *yys1;
      int yyn;
      for (yys0 = yyy0->yystate, yys1 = yyy1->yystate,
           yyn = yyrhsLength (yyy0->yyrule);
           yyn > 0;
           yys0 = yys0->yypred, yys1 = yys1->yypred, yyn -= 1)
        if (yys0->yyposn != yys1->yyposn)
          return yyfalse;
      return yytrue;
    }
  else
    return yyfalse;
}
/** Assuming identicalOptions (YYY0,YYY1), destructively merge the
 *  alternative semantic values for the RHS-symbols of YYY1 and YYY0.  */
static void
yymergeOptionSets (yySemanticOption* yyy0, yySemanticOption* yyy1)
{
  yyGLRState *yys0, *yys1;
  int yyn;
  for (yys0 = yyy0->yystate, yys1 = yyy1->yystate,
       yyn = yyrhsLength (yyy0->yyrule);
       0 < yyn;
       yys0 = yys0->yypred, yys1 = yys1->yypred, yyn -= 1)
    {
      if (yys0 == yys1)
        break;
      else if (yys0->yyresolved)
        {
          yys1->yyresolved = yytrue;
          yys1->yysemantics.yyval = yys0->yysemantics.yyval;
        }
      else if (yys1->yyresolved)
        {
          yys0->yyresolved = yytrue;
          yys0->yysemantics.yyval = yys1->yysemantics.yyval;
        }
      else
        {
          yySemanticOption** yyz0p = &yys0->yysemantics.yyfirstVal;
          yySemanticOption* yyz1 = yys1->yysemantics.yyfirstVal;
          while (yytrue)
            {
              if (yyz1 == *yyz0p || yyz1 == YY_NULLPTR)
                break;
              else if (*yyz0p == YY_NULLPTR)
                {
                  *yyz0p = yyz1;
                  break;
                }
              else if (*yyz0p < yyz1)
                {
                  yySemanticOption* yyz = *yyz0p;
                  *yyz0p = yyz1;
                  yyz1 = yyz1->yynext;
                  (*yyz0p)->yynext = yyz;
                }
              yyz0p = &(*yyz0p)->yynext;
            }
          yys1->yysemantics.yyfirstVal = yys0->yysemantics.yyfirstVal;
        }
    }
}
/** Y0 and Y1 represent two possible actions to take in a given
 *  parsing state; return 0 if no combination is possible,
 *  1 if user-mergeable, 2 if Y0 is preferred, 3 if Y1 is preferred.  */
static int
yypreference (yySemanticOption* y0, yySemanticOption* y1)
{
  yyRuleNum r0 = y0->yyrule, r1 = y1->yyrule;
  int p0 = yydprec[r0], p1 = yydprec[r1];
  if (p0 == p1)
    {
      if (yymerger[r0] == 0 || yymerger[r0] != yymerger[r1])
        return 0;
      else
        return 1;
    }
  if (p0 == 0 || p1 == 0)
    return 0;
  if (p0 < p1)
    return 3;
  if (p1 < p0)
    return 2;
  return 0;
}
static YYRESULTTAG
yyresolveValue (yyGLRState* yys, yyGLRStack* yystackp, mc_value* mcast);
/** Resolve the previous YYN states starting at and including state YYS
 *  on *YYSTACKP. If result != yyok, some states may have been left
 *  unresolved possibly with empty semantic option chains.  Regardless
 *  of whether result = yyok, each state has been left with consistent
 *  data so that yydestroyGLRState can be invoked if necessary.  */
static YYRESULTTAG
yyresolveStates (yyGLRState* yys, int yyn,
                 yyGLRStack* yystackp, mc_value* mcast)
{
  if (0 < yyn)
    {
      YY_ASSERT (yys->yypred);
      YYCHK (yyresolveStates (yys->yypred, yyn-1, yystackp, mcast));
      if (! yys->yyresolved)
        YYCHK (yyresolveValue (yys, yystackp, mcast));
    }
  return yyok;
}
/** Resolve the states for the RHS of YYOPT on *YYSTACKP, perform its
 *  user action, and return the semantic value and location in *YYVALP
 *  and *YYLOCP.  Regardless of whether result = yyok, all RHS states
 *  have been destroyed (assuming the user action destroys all RHS
 *  semantic values if invoked).  */
static YYRESULTTAG
yyresolveAction (yySemanticOption* yyopt, yyGLRStack* yystackp,
                 YYSTYPE* yyvalp, YYLTYPE *yylocp, mc_value* mcast)
{
  yyGLRStackItem yyrhsVals[YYMAXRHS + YYMAXLEFT + 1];
  int yynrhs = yyrhsLength (yyopt->yyrule);
  YYRESULTTAG yyflag =
    yyresolveStates (yyopt->yystate, yynrhs, yystackp, mcast);
  if (yyflag != yyok)
    {
      yyGLRState *yys;
      for (yys = yyopt->yystate; yynrhs > 0; yys = yys->yypred, yynrhs -= 1)
        yydestroyGLRState ("Cleanup: popping", yys, mcast);
      return yyflag;
    }
  yyrhsVals[YYMAXRHS + YYMAXLEFT].yystate.yypred = yyopt->yystate;
  if (yynrhs == 0)
    /* Set default location.  */
    yyrhsVals[YYMAXRHS + YYMAXLEFT - 1].yystate.yyloc = yyopt->yystate->yyloc;
  {
    int yychar_current = yychar;
    YYSTYPE yylval_current = yylval;
    YYLTYPE yylloc_current = yylloc;
    yychar = yyopt->yyrawchar;
    yylval = yyopt->yyval;
    yylloc = yyopt->yyloc;
    yyflag = yyuserAction (yyopt->yyrule, yynrhs,
                           yyrhsVals + YYMAXRHS + YYMAXLEFT - 1,
                           yystackp, -1, yyvalp, yylocp, mcast);
    yychar = yychar_current;
    yylval = yylval_current;
    yylloc = yylloc_current;
  }
  return yyflag;
}
#if MCA_DEBUG
static void
yyreportTree (yySemanticOption* yyx, int yyindent)
{
  int yynrhs = yyrhsLength (yyx->yyrule);
  int yyi;
  yyGLRState* yys;
  yyGLRState* yystates[1 + YYMAXRHS];
  yyGLRState yyleftmost_state;
  for (yyi = yynrhs, yys = yyx->yystate; 0 < yyi; yyi -= 1, yys = yys->yypred)
    yystates[yyi] = yys;
  if (yys == YY_NULLPTR)
    {
      yyleftmost_state.yyposn = 0;
      yystates[0] = &yyleftmost_state;
    }
  else
    yystates[0] = yys;
  if (yyx->yystate->yyposn < yys->yyposn + 1)
    YY_FPRINTF ((stderr, "%*s%s -> <Rule %d, empty>\n",
                 yyindent, "", yysymbol_name (yylhsNonterm (yyx->yyrule)),
                 yyx->yyrule - 1));
  else
    YY_FPRINTF ((stderr, "%*s%s -> <Rule %d, tokens %ld .. %ld>\n",
                 yyindent, "", yysymbol_name (yylhsNonterm (yyx->yyrule)),
                 yyx->yyrule - 1, YY_CAST (long, yys->yyposn + 1),
                 YY_CAST (long, yyx->yystate->yyposn)));
  for (yyi = 1; yyi <= yynrhs; yyi += 1)
    {
      if (yystates[yyi]->yyresolved)
        {
          if (yystates[yyi-1]->yyposn+1 > yystates[yyi]->yyposn)
            YY_FPRINTF ((stderr, "%*s%s <empty>\n", yyindent+2, "",
                         yysymbol_name (yy_accessing_symbol (yystates[yyi]->yylrState))));
          else
            YY_FPRINTF ((stderr, "%*s%s <tokens %ld .. %ld>\n", yyindent+2, "",
                         yysymbol_name (yy_accessing_symbol (yystates[yyi]->yylrState)),
                         YY_CAST (long, yystates[yyi-1]->yyposn + 1),
                         YY_CAST (long, yystates[yyi]->yyposn)));
        }
      else
        yyreportTree (yystates[yyi]->yysemantics.yyfirstVal, yyindent+2);
    }
}
#endif
static YYRESULTTAG
yyreportAmbiguity (yySemanticOption* yyx0,
                   yySemanticOption* yyx1, YYLTYPE *yylocp, mc_value* mcast)
{
  YY_USE (yyx0);
  YY_USE (yyx1);
#if MCA_DEBUG
  YY_FPRINTF ((stderr, "Ambiguity detected.\n"));
  YY_FPRINTF ((stderr, "Option 1,\n"));
  yyreportTree (yyx0, 2);
  YY_FPRINTF ((stderr, "\nOption 2,\n"));
  yyreportTree (yyx1, 2);
  YY_FPRINTF ((stderr, "\n"));
#endif
  yyerror (yylocp, mcast, YY_("syntax is ambiguous"));
  return yyabort;
}
/** Resolve the locations for each of the YYN1 states in *YYSTACKP,
 *  ending at YYS1.  Has no effect on previously resolved states.
 *  The first semantic option of a state is always chosen.  */
static void
yyresolveLocations (yyGLRState *yys1, int yyn1,
                    yyGLRStack *yystackp, mc_value* mcast)
{
  if (0 < yyn1)
    {
      yyresolveLocations (yys1->yypred, yyn1 - 1, yystackp, mcast);
      if (!yys1->yyresolved)
        {
          yyGLRStackItem yyrhsloc[1 + YYMAXRHS];
          int yynrhs;
          yySemanticOption *yyoption = yys1->yysemantics.yyfirstVal;
          YY_ASSERT (yyoption);
          yynrhs = yyrhsLength (yyoption->yyrule);
          if (0 < yynrhs)
            {
              yyGLRState *yys;
              int yyn;
              yyresolveLocations (yyoption->yystate, yynrhs,
                                  yystackp, mcast);
              for (yys = yyoption->yystate, yyn = yynrhs;
                   yyn > 0;
                   yys = yys->yypred, yyn -= 1)
                yyrhsloc[yyn].yystate.yyloc = yys->yyloc;
            }
          else
            {
              /* Both yyresolveAction and yyresolveLocations traverse the GSS
                 in reverse rightmost order.  It is only necessary to invoke
                 yyresolveLocations on a subforest for which yyresolveAction
                 would have been invoked next had an ambiguity not been
                 detected.  Thus the location of the previous state (but not
                 necessarily the previous state itself) is guaranteed to be
                 resolved already.  */
              yyGLRState *yyprevious = yyoption->yystate;
              yyrhsloc[0].yystate.yyloc = yyprevious->yyloc;
            }
          YYLLOC_DEFAULT ((yys1->yyloc), yyrhsloc, yynrhs);
        }
    }
}
/** Resolve the ambiguity represented in state YYS in *YYSTACKP,
 *  perform the indicated actions, and set the semantic value of YYS.
 *  If result != yyok, the chain of semantic options in YYS has been
 *  cleared instead or it has been left unmodified except that
 *  redundant options may have been removed.  Regardless of whether
 *  result = yyok, YYS has been left with consistent data so that
 *  yydestroyGLRState can be invoked if necessary.  */
static YYRESULTTAG
yyresolveValue (yyGLRState* yys, yyGLRStack* yystackp, mc_value* mcast)
{
  yySemanticOption* yyoptionList = yys->yysemantics.yyfirstVal;
  yySemanticOption* yybest = yyoptionList;
  yySemanticOption** yypp;
  yybool yymerge = yyfalse;
  YYSTYPE yyval;
  YYRESULTTAG yyflag;
  YYLTYPE *yylocp = &yys->yyloc;
  for (yypp = &yyoptionList->yynext; *yypp != YY_NULLPTR; )
    {
      yySemanticOption* yyp = *yypp;
      if (yyidenticalOptions (yybest, yyp))
        {
          yymergeOptionSets (yybest, yyp);
          *yypp = yyp->yynext;
        }
      else
        {
          switch (yypreference (yybest, yyp))
            {
            case 0:
              yyresolveLocations (yys, 1, yystackp, mcast);
              return yyreportAmbiguity (yybest, yyp, yylocp, mcast);
              break;
            case 1:
              yymerge = yytrue;
              break;
            case 2:
              break;
            case 3:
              yybest = yyp;
              yymerge = yyfalse;
              break;
            default:
              /* This cannot happen so it is not worth a YY_ASSERT (yyfalse),
                 but some compilers complain if the default case is
                 omitted.  */
              break;
            }
          yypp = &yyp->yynext;
        }
    }
  if (yymerge)
    {
      yySemanticOption* yyp;
      int yyprec = yydprec[yybest->yyrule];
      yyflag = yyresolveAction (yybest, yystackp, &yyval, yylocp, mcast);
      if (yyflag == yyok)
        for (yyp = yybest->yynext; yyp != YY_NULLPTR; yyp = yyp->yynext)
          {
            if (yyprec == yydprec[yyp->yyrule])
              {
                YYSTYPE yyval_other;
                YYLTYPE yydummy;
                yyflag = yyresolveAction (yyp, yystackp, &yyval_other, &yydummy, mcast);
                if (yyflag != yyok)
                  {
                    yydestruct ("Cleanup: discarding incompletely merged value for",
                                yy_accessing_symbol (yys->yylrState),
                                &yyval, yylocp, mcast);
                    break;
                  }
                yyuserMerge (yymerger[yyp->yyrule], &yyval, &yyval_other);
              }
          }
    }
  else
    yyflag = yyresolveAction (yybest, yystackp, &yyval, yylocp, mcast);
  if (yyflag == yyok)
    {
      yys->yyresolved = yytrue;
      yys->yysemantics.yyval = yyval;
    }
  else
    yys->yysemantics.yyfirstVal = YY_NULLPTR;
  return yyflag;
}
static YYRESULTTAG
yyresolveStack (yyGLRStack* yystackp, mc_value* mcast)
{
  if (yystackp->yysplitPoint != YY_NULLPTR)
    {
      yyGLRState* yys;
      int yyn;
      for (yyn = 0, yys = yystackp->yytops.yystates[0];
           yys != yystackp->yysplitPoint;
           yys = yys->yypred, yyn += 1)
        continue;
      YYCHK (yyresolveStates (yystackp->yytops.yystates[0], yyn, yystackp
                             , mcast));
    }
  return yyok;
}
/** Called when returning to deterministic operation to clean up the extra
 * stacks. */
static void
yycompressStack (yyGLRStack* yystackp)
{
  /* yyr is the state after the split point.  */
  yyGLRState *yyr;
  if (yystackp->yytops.yysize != 1 || yystackp->yysplitPoint == YY_NULLPTR)
    return;
  {
    yyGLRState *yyp, *yyq;
    for (yyp = yystackp->yytops.yystates[0], yyq = yyp->yypred, yyr = YY_NULLPTR;
         yyp != yystackp->yysplitPoint;
         yyr = yyp, yyp = yyq, yyq = yyp->yypred)
      yyp->yypred = yyr;
  }
  yystackp->yyspaceLeft += yystackp->yynextFree - yystackp->yyitems;
  yystackp->yynextFree = YY_REINTERPRET_CAST (yyGLRStackItem*, yystackp->yysplitPoint) + 1;
  yystackp->yyspaceLeft -= yystackp->yynextFree - yystackp->yyitems;
  yystackp->yysplitPoint = YY_NULLPTR;
  yystackp->yylastDeleted = YY_NULLPTR;
  while (yyr != YY_NULLPTR)
    {
      yystackp->yynextFree->yystate = *yyr;
      yyr = yyr->yypred;
      yystackp->yynextFree->yystate.yypred = &yystackp->yynextFree[-1].yystate;
      yystackp->yytops.yystates[0] = &yystackp->yynextFree->yystate;
      yystackp->yynextFree += 1;
      yystackp->yyspaceLeft -= 1;
    }
}
static YYRESULTTAG
yyprocessOneStack (yyGLRStack* yystackp, YYPTRDIFF_T yyk,
                   YYPTRDIFF_T yyposn, YYLTYPE *yylocp, mc_value* mcast)
{
  while (yystackp->yytops.yystates[yyk] != YY_NULLPTR)
    {
      yy_state_t yystate = yystackp->yytops.yystates[yyk]->yylrState;
      YY_DPRINTF ((stderr, "Stack %ld Entering state %d\n",
                   YY_CAST (long, yyk), yystate));
      YY_ASSERT (yystate != YYFINAL);
      if (yyisDefaultedState (yystate))
        {
          YYRESULTTAG yyflag;
          yyRuleNum yyrule = yydefaultAction (yystate);
          if (yyrule == 0)
            {
              YY_DPRINTF ((stderr, "Stack %ld dies.\n", YY_CAST (long, yyk)));
              yymarkStackDeleted (yystackp, yyk);
              return yyok;
            }
          yyflag = yyglrReduce (yystackp, yyk, yyrule, yyimmediate[yyrule], mcast);
          if (yyflag == yyerr)
            {
              YY_DPRINTF ((stderr,
                           "Stack %ld dies "
                           "(predicate failure or explicit user error).\n",
                           YY_CAST (long, yyk)));
              yymarkStackDeleted (yystackp, yyk);
              return yyok;
            }
          if (yyflag != yyok)
            return yyflag;
        }
      else
        {
          yysymbol_kind_t yytoken = yygetToken (&yychar, yystackp, mcast);
          const short* yyconflicts;
          const int yyaction = yygetLRActions (yystate, yytoken, &yyconflicts);
          yystackp->yytops.yylookaheadNeeds[yyk] = yytrue;
          for (/* nothing */; *yyconflicts; yyconflicts += 1)
            {
              YYRESULTTAG yyflag;
              YYPTRDIFF_T yynewStack = yysplitStack (yystackp, yyk);
              YY_DPRINTF ((stderr, "Splitting off stack %ld from %ld.\n",
                           YY_CAST (long, yynewStack), YY_CAST (long, yyk)));
              yyflag = yyglrReduce (yystackp, yynewStack,
                                    *yyconflicts,
                                    yyimmediate[*yyconflicts], mcast);
              if (yyflag == yyok)
                YYCHK (yyprocessOneStack (yystackp, yynewStack,
                                          yyposn, yylocp, mcast));
              else if (yyflag == yyerr)
                {
                  YY_DPRINTF ((stderr, "Stack %ld dies.\n", YY_CAST (long, yynewStack)));
                  yymarkStackDeleted (yystackp, yynewStack);
                }
              else
                return yyflag;
            }
          if (yyisShiftAction (yyaction))
            break;
          else if (yyisErrorAction (yyaction))
            {
              YY_DPRINTF ((stderr, "Stack %ld dies.\n", YY_CAST (long, yyk)));
              yymarkStackDeleted (yystackp, yyk);
              break;
            }
          else
            {
              YYRESULTTAG yyflag = yyglrReduce (yystackp, yyk, -yyaction,
                                                yyimmediate[-yyaction], mcast);
              if (yyflag == yyerr)
                {
                  YY_DPRINTF ((stderr,
                               "Stack %ld dies "
                               "(predicate failure or explicit user error).\n",
                               YY_CAST (long, yyk)));
                  yymarkStackDeleted (yystackp, yyk);
                  break;
                }
              else if (yyflag != yyok)
                return yyflag;
            }
        }
    }
  return yyok;
}
static void
yyreportSyntaxError (yyGLRStack* yystackp, mc_value* mcast)
{
  if (yystackp->yyerrState != 0)
    return;
  yyerror (&yylloc, mcast, YY_("syntax error"));
  yynerrs += 1;
}
/* Recover from a syntax error on *YYSTACKP, assuming that *YYSTACKP->YYTOKENP,
   yylval, and yylloc are the syntactic category, semantic value, and location
   of the lookahead.  */
static void
yyrecoverSyntaxError (yyGLRStack* yystackp, mc_value* mcast)
{
  if (yystackp->yyerrState == 3)
    /* We just shifted the error token and (perhaps) took some
       reductions.  Skip tokens until we can proceed.  */
    while (yytrue)
      {
        yysymbol_kind_t yytoken;
        int yyj;
        if (yychar == MCA_EOF)
          yyFail (yystackp, &yylloc, mcast, YY_NULLPTR);
        if (yychar != MCA_EMPTY)
          {
            /* We throw away the lookahead, but the error range
               of the shifted error token must take it into account.  */
            yyGLRState *yys = yystackp->yytops.yystates[0];
            yyGLRStackItem yyerror_range[3];
            yyerror_range[1].yystate.yyloc = yys->yyloc;
            yyerror_range[2].yystate.yyloc = yylloc;
            YYLLOC_DEFAULT ((yys->yyloc), yyerror_range, 2);
            yytoken = YYTRANSLATE (yychar);
            yydestruct ("Error: discarding",
                        yytoken, &yylval, &yylloc, mcast);
            yychar = MCA_EMPTY;
          }
        yytoken = yygetToken (&yychar, yystackp, mcast);
        yyj = yypact[yystackp->yytops.yystates[0]->yylrState];
        if (yypact_value_is_default (yyj))
          return;
        yyj += yytoken;
        if (yyj < 0 || YYLAST < yyj || yycheck[yyj] != yytoken)
          {
            if (yydefact[yystackp->yytops.yystates[0]->yylrState] != 0)
              return;
          }
        else if (! yytable_value_is_error (yytable[yyj]))
          return;
      }
  /* Reduce to one stack.  */
  {
    YYPTRDIFF_T yyk;
    for (yyk = 0; yyk < yystackp->yytops.yysize; yyk += 1)
      if (yystackp->yytops.yystates[yyk] != YY_NULLPTR)
        break;
    if (yyk >= yystackp->yytops.yysize)
      yyFail (yystackp, &yylloc, mcast, YY_NULLPTR);
    for (yyk += 1; yyk < yystackp->yytops.yysize; yyk += 1)
      yymarkStackDeleted (yystackp, yyk);
    yyremoveDeletes (yystackp);
    yycompressStack (yystackp);
  }
  /* Pop stack until we find a state that shifts the error token.  */
  yystackp->yyerrState = 3;
  while (yystackp->yytops.yystates[0] != YY_NULLPTR)
    {
      yyGLRState *yys = yystackp->yytops.yystates[0];
      int yyj = yypact[yys->yylrState];
      if (! yypact_value_is_default (yyj))
        {
          yyj += YYSYMBOL_YYerror;
          if (0 <= yyj && yyj <= YYLAST && yycheck[yyj] == YYSYMBOL_YYerror
              && yyisShiftAction (yytable[yyj]))
            {
              /* Shift the error token.  */
              int yyaction = yytable[yyj];
              /* First adjust its location.*/
              YYLTYPE yyerrloc;
              yystackp->yyerror_range[2].yystate.yyloc = yylloc;
              YYLLOC_DEFAULT (yyerrloc, (yystackp->yyerror_range), 2);
              YY_SYMBOL_PRINT ("Shifting", yy_accessing_symbol (yyaction),
                               &yylval, &yyerrloc);
              yyglrShift (yystackp, 0, yyaction,
                          yys->yyposn, &yylval, &yyerrloc);
              yys = yystackp->yytops.yystates[0];
              break;
            }
        }
      yystackp->yyerror_range[1].yystate.yyloc = yys->yyloc;
      if (yys->yypred != YY_NULLPTR)
        yydestroyGLRState ("Error: popping", yys, mcast);
      yystackp->yytops.yystates[0] = yys->yypred;
      yystackp->yynextFree -= 1;
      yystackp->yyspaceLeft += 1;
    }
  if (yystackp->yytops.yystates[0] == YY_NULLPTR)
    yyFail (yystackp, &yylloc, mcast, YY_NULLPTR);
}
#define YYCHK1(YYE)                             \
  do {                                          \
    switch (YYE) {                              \
    case yyok:     break;                       \
    case yyabort:  goto yyabortlab;             \
    case yyaccept: goto yyacceptlab;            \
    case yyerr:    goto yyuser_error;           \
    case yynomem:  goto yyexhaustedlab;         \
    default:       goto yybuglab;               \
    }                                           \
  } while (0)
/*----------.
| yyparse.  |
`----------*/
int
yyparse (mc_value* mcast)
{
  int yyresult;
  yyGLRStack yystack;
  yyGLRStack* const yystackp = &yystack;
  YYPTRDIFF_T yyposn;
  YY_DPRINTF ((stderr, "Starting parse\n"));
  yychar = MCA_EMPTY;
  yylval = yyval_default;
  yylloc = yyloc_default;
  if (! yyinitGLRStack (yystackp, YYINITDEPTH))
    goto yyexhaustedlab;
  switch (YYSETJMP (yystack.yyexception_buffer))
    {
    case 0: break;
    case 1: goto yyabortlab;
    case 2: goto yyexhaustedlab;
    default: goto yybuglab;
    }
  yyglrShift (&yystack, 0, 0, 0, &yylval, &yylloc);
  yyposn = 0;
  while (yytrue)
    {
      /* For efficiency, we have two loops, the first of which is
         specialized to deterministic operation (single stack, no
         potential ambiguity).  */
      /* Standard mode. */
      while (yytrue)
        {
          yy_state_t yystate = yystack.yytops.yystates[0]->yylrState;
          YY_DPRINTF ((stderr, "Entering state %d\n", yystate));
          if (yystate == YYFINAL)
            goto yyacceptlab;
          if (yyisDefaultedState (yystate))
            {
              yyRuleNum yyrule = yydefaultAction (yystate);
              if (yyrule == 0)
                {
                  yystack.yyerror_range[1].yystate.yyloc = yylloc;
                  yyreportSyntaxError (&yystack, mcast);
                  goto yyuser_error;
                }
              YYCHK1 (yyglrReduce (&yystack, 0, yyrule, yytrue, mcast));
            }
          else
            {
              yysymbol_kind_t yytoken = yygetToken (&yychar, yystackp, mcast);
              const short* yyconflicts;
              int yyaction = yygetLRActions (yystate, yytoken, &yyconflicts);
              if (*yyconflicts)
                /* Enter nondeterministic mode.  */
                break;
              if (yyisShiftAction (yyaction))
                {
                  YY_SYMBOL_PRINT ("Shifting", yytoken, &yylval, &yylloc);
                  yychar = MCA_EMPTY;
                  yyposn += 1;
                  yyglrShift (&yystack, 0, yyaction, yyposn, &yylval, &yylloc);
                  if (0 < yystack.yyerrState)
                    yystack.yyerrState -= 1;
                }
              else if (yyisErrorAction (yyaction))
                {
                  yystack.yyerror_range[1].yystate.yyloc = yylloc;
                  /* Issue an error message unless the scanner already
                     did. */
                  if (yychar != MCA_error)
                    yyreportSyntaxError (&yystack, mcast);
                  goto yyuser_error;
                }
              else
                YYCHK1 (yyglrReduce (&yystack, 0, -yyaction, yytrue, mcast));
            }
        }
      /* Nondeterministic mode. */
      while (yytrue)
        {
          yysymbol_kind_t yytoken_to_shift;
          YYPTRDIFF_T yys;
          for (yys = 0; yys < yystack.yytops.yysize; yys += 1)
            yystackp->yytops.yylookaheadNeeds[yys] = yychar != MCA_EMPTY;
          /* yyprocessOneStack returns one of three things:
              - An error flag.  If the caller is yyprocessOneStack, it
                immediately returns as well.  When the caller is finally
                yyparse, it jumps to an error label via YYCHK1.
              - yyok, but yyprocessOneStack has invoked yymarkStackDeleted
                (&yystack, yys), which sets the top state of yys to NULL.  Thus,
                yyparse's following invocation of yyremoveDeletes will remove
                the stack.
              - yyok, when ready to shift a token.
             Except in the first case, yyparse will invoke yyremoveDeletes and
             then shift the next token onto all remaining stacks.  This
             synchronization of the shift (that is, after all preceding
             reductions on all stacks) helps prevent double destructor calls
             on yylval in the event of memory exhaustion.  */
          for (yys = 0; yys < yystack.yytops.yysize; yys += 1)
            YYCHK1 (yyprocessOneStack (&yystack, yys, yyposn, &yylloc, mcast));
          yyremoveDeletes (&yystack);
          if (yystack.yytops.yysize == 0)
            {
              yyundeleteLastStack (&yystack);
              if (yystack.yytops.yysize == 0)
                yyFail (&yystack, &yylloc, mcast, YY_("syntax error"));
              YYCHK1 (yyresolveStack (&yystack, mcast));
              YY_DPRINTF ((stderr, "Returning to deterministic operation.\n"));
              yystack.yyerror_range[1].yystate.yyloc = yylloc;
              yyreportSyntaxError (&yystack, mcast);
              goto yyuser_error;
            }
          /* If any yyglrShift call fails, it will fail after shifting.  Thus,
             a copy of yylval will already be on stack 0 in the event of a
             failure in the following loop.  Thus, yychar is set to MCA_EMPTY
             before the loop to make sure the user destructor for yylval isn't
             called twice.  */
          yytoken_to_shift = YYTRANSLATE (yychar);
          yychar = MCA_EMPTY;
          yyposn += 1;
          for (yys = 0; yys < yystack.yytops.yysize; yys += 1)
            {
              yy_state_t yystate = yystack.yytops.yystates[yys]->yylrState;
              const short* yyconflicts;
              int yyaction = yygetLRActions (yystate, yytoken_to_shift,
                              &yyconflicts);
              /* Note that yyconflicts were handled by yyprocessOneStack.  */
              YY_DPRINTF ((stderr, "On stack %ld, ", YY_CAST (long, yys)));
              YY_SYMBOL_PRINT ("shifting", yytoken_to_shift, &yylval, &yylloc);
              yyglrShift (&yystack, yys, yyaction, yyposn,
                          &yylval, &yylloc);
              YY_DPRINTF ((stderr, "Stack %ld now in state %d\n",
                           YY_CAST (long, yys),
                           yystack.yytops.yystates[yys]->yylrState));
            }
          if (yystack.yytops.yysize == 1)
            {
              YYCHK1 (yyresolveStack (&yystack, mcast));
              YY_DPRINTF ((stderr, "Returning to deterministic operation.\n"));
              yycompressStack (&yystack);
              break;
            }
        }
      continue;
    yyuser_error:
      yyrecoverSyntaxError (&yystack, mcast);
      yyposn = yystack.yytops.yystates[0]->yyposn;
    }
 yyacceptlab:
  yyresult = 0;
  goto yyreturnlab;
 yybuglab:
  YY_ASSERT (yyfalse);
  goto yyabortlab;
 yyabortlab:
  yyresult = 1;
  goto yyreturnlab;
 yyexhaustedlab:
  yyerror (&yylloc, mcast, YY_("memory exhausted"));
  yyresult = 2;
  goto yyreturnlab;
 yyreturnlab:
  if (yychar != MCA_EMPTY)
    yydestruct ("Cleanup: discarding lookahead",
                YYTRANSLATE (yychar), &yylval, &yylloc, mcast);
  /* If the stack is well-formed, pop the stack until it is empty,
     destroying its entries as we go.  But free the stack regardless
     of whether it is well-formed.  */
  if (yystack.yyitems)
    {
      yyGLRState** yystates = yystack.yytops.yystates;
      if (yystates)
        {
          YYPTRDIFF_T yysize = yystack.yytops.yysize;
          YYPTRDIFF_T yyk;
          for (yyk = 0; yyk < yysize; yyk += 1)
            if (yystates[yyk])
              {
                while (yystates[yyk])
                  {
                    yyGLRState *yys = yystates[yyk];
                    yystack.yyerror_range[1].yystate.yyloc = yys->yyloc;
                    if (yys->yypred != YY_NULLPTR)
                      yydestroyGLRState ("Cleanup: popping", yys, mcast);
                    yystates[yyk] = yys->yypred;
                    yystack.yynextFree -= 1;
                    yystack.yyspaceLeft += 1;
                  }
                break;
              }
        }
      yyfreeGLRStack (&yystack);
    }
  return yyresult;
}
/* DEBUGGING ONLY */
#if MCA_DEBUG
/* Print *YYS and its predecessors. */
static void
yy_yypstack (yyGLRState* yys)
{
  if (yys->yypred)
    {
      yy_yypstack (yys->yypred);
      YY_FPRINTF ((stderr, " -> "));
    }
  YY_FPRINTF ((stderr, "%d@%ld", yys->yylrState, YY_CAST (long, yys->yyposn)));
}
/* Print YYS (possibly NULL) and its predecessors. */
static void
yypstates (yyGLRState* yys)
{
  if (yys == YY_NULLPTR)
    YY_FPRINTF ((stderr, "<null>"));
  else
    yy_yypstack (yys);
  YY_FPRINTF ((stderr, "\n"));
}
/* Print the stack #YYK.  */
static void
yypstack (yyGLRStack* yystackp, YYPTRDIFF_T yyk)
{
  yypstates (yystackp->yytops.yystates[yyk]);
}
/* Print all the stacks.  */
static void
yypdumpstack (yyGLRStack* yystackp)
{
#define YYINDEX(YYX)                                                    \
  YY_CAST (long,                                                        \
           ((YYX)                                                       \
            ? YY_REINTERPRET_CAST (yyGLRStackItem*, (YYX)) - yystackp->yyitems \
            : -1))
  yyGLRStackItem* yyp;
  for (yyp = yystackp->yyitems; yyp < yystackp->yynextFree; yyp += 1)
    {
      YY_FPRINTF ((stderr, "%3ld. ",
                   YY_CAST (long, yyp - yystackp->yyitems)));
      if (*YY_REINTERPRET_CAST (yybool *, yyp))
        {
          YY_ASSERT (yyp->yystate.yyisState);
          YY_ASSERT (yyp->yyoption.yyisState);
          YY_FPRINTF ((stderr, "Res: %d, LR State: %d, posn: %ld, pred: %ld",
                       yyp->yystate.yyresolved, yyp->yystate.yylrState,
                       YY_CAST (long, yyp->yystate.yyposn),
                       YYINDEX (yyp->yystate.yypred)));
          if (! yyp->yystate.yyresolved)
            YY_FPRINTF ((stderr, ", firstVal: %ld",
                         YYINDEX (yyp->yystate.yysemantics.yyfirstVal)));
        }
      else
        {
          YY_ASSERT (!yyp->yystate.yyisState);
          YY_ASSERT (!yyp->yyoption.yyisState);
          YY_FPRINTF ((stderr, "Option. rule: %d, state: %ld, next: %ld",
                       yyp->yyoption.yyrule - 1,
                       YYINDEX (yyp->yyoption.yystate),
                       YYINDEX (yyp->yyoption.yynext)));
        }
      YY_FPRINTF ((stderr, "\n"));
    }
  YY_FPRINTF ((stderr, "Tops:"));
  {
    YYPTRDIFF_T yyi;
    for (yyi = 0; yyi < yystackp->yytops.yysize; yyi += 1)
      YY_FPRINTF ((stderr, "%ld: %ld; ", YY_CAST (long, yyi),
                   YYINDEX (yystackp->yytops.yystates[yyi])));
    YY_FPRINTF ((stderr, "\n"));
  }
#undef YYINDEX
}
#endif
#undef yylval
#undef yychar
#undef yynerrs
#undef yylloc
/* Substitute the variable and function names.  */
#define yyparse mca_parse
#define yylex   mca_lex
#define yyerror mca_error
#define yylval  mca_lval
#define yychar  mca_char
#define yydebug mca_debug
#define yynerrs mca_nerrs
#define yylloc  mca_lloc
void mca_error(struct YYLTYPE *_loc, mc_value* mcast, const char *msg) {
    (void)_loc;
    (void)mcast;
    (void)msg;
    // All syntax errors are now caught by explicit error recovery rules
    // (E1002-E1031) with precise position spans and specific messages.
    // The generic E1000 fallback is no longer needed — silence GLR-internal
    // "syntax is ambiguous" noise and let the error rules do their job.
}
