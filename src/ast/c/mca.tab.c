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
#line 5 "src/mca.y"
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
#line 93 "src/mca.tab.c"
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
  YYSYMBOL_MCK_DEFINE = 24,                /* MCK_DEFINE  */
  YYSYMBOL_MCK_CAPABILITY = 25,            /* MCK_CAPABILITY  */
  YYSYMBOL_MCK_ABSTRACT = 26,              /* MCK_ABSTRACT  */
  YYSYMBOL_MCPT_SEMICOLON = 27,            /* MCPT_SEMICOLON  */
  YYSYMBOL_MCPT_COMMA = 28,                /* MCPT_COMMA  */
  YYSYMBOL_MCK_ROLE = 29,                  /* MCK_ROLE  */
  YYSYMBOL_MCK_CONDUIT = 30,               /* MCK_CONDUIT  */
  YYSYMBOL_MCK_DOMAIN = 31,                /* MCK_DOMAIN  */
  YYSYMBOL_MCK_RAIL = 32,                  /* MCK_RAIL  */
  YYSYMBOL_MCK_PARTITION = 33,             /* MCK_PARTITION  */
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
  YYSYMBOL_MCK_IO = 70,                    /* MCK_IO  */
  YYSYMBOL_MCK_IN = 71,                    /* MCK_IN  */
  YYSYMBOL_MCK_OUT = 72,                   /* MCK_OUT  */
  YYSYMBOL_MCK_NC = 73,                    /* MCK_NC  */
  YYSYMBOL_MCK_LABEL = 74,                 /* MCK_LABEL  */
  YYSYMBOL_MCK_PSRC = 75,                  /* MCK_PSRC  */
  YYSYMBOL_MCK_PSNK = 76,                  /* MCK_PSNK  */
  YYSYMBOL_MCK_PSBI = 77,                  /* MCK_PSBI  */
  YYSYMBOL_MCONST_HIGH = 78,               /* MCONST_HIGH  */
  YYSYMBOL_MCONST_LOW = 79,                /* MCONST_LOW  */
  YYSYMBOL_MCONST_NC = 80,                 /* MCONST_NC  */
  YYSYMBOL_MCU_INT = 81,                   /* MCU_INT  */
  YYSYMBOL_MCU_HEX = 82,                   /* MCU_HEX  */
  YYSYMBOL_MCU_FLOAT = 83,                 /* MCU_FLOAT  */
  YYSYMBOL_MCU_STRING = 84,                /* MCU_STRING  */
  YYSYMBOL_MCK_FUNC = 85,                  /* MCK_FUNC  */
  YYSYMBOL_MCK_THIS = 86,                  /* MCK_THIS  */
  YYSYMBOL_MCU_VOLT = 87,                  /* MCU_VOLT  */
  YYSYMBOL_MCU_AMP = 88,                   /* MCU_AMP  */
  YYSYMBOL_MCU_CAP = 89,                   /* MCU_CAP  */
  YYSYMBOL_MCU_IND = 90,                   /* MCU_IND  */
  YYSYMBOL_MCU_TIME = 91,                  /* MCU_TIME  */
  YYSYMBOL_MCU_LEN = 92,                   /* MCU_LEN  */
  YYSYMBOL_MCU_WATT = 93,                  /* MCU_WATT  */
  YYSYMBOL_MCU_OHM = 94,                   /* MCU_OHM  */
  YYSYMBOL_MCU_TEMP = 95,                  /* MCU_TEMP  */
  YYSYMBOL_MCU_HZ = 96,                    /* MCU_HZ  */
  YYSYMBOL_MCU_DB = 97,                    /* MCU_DB  */
  YYSYMBOL_MCU_PPM = 98,                   /* MCU_PPM  */
  YYSYMBOL_MCU_PERCENT = 99,               /* MCU_PERCENT  */
  YYSYMBOL_MCU_BAUD = 100,                 /* MCU_BAUD  */
  YYSYMBOL_MCU_DATASIZE = 101,             /* MCU_DATASIZE  */
  YYSYMBOL_MCU_SPS = 102,                  /* MCU_SPS  */
  YYSYMBOL_MCU_SIEMENS = 103,              /* MCU_SIEMENS  */
  YYSYMBOL_MCU_RESPONSIVITY = 104,         /* MCU_RESPONSIVITY  */
  YYSYMBOL_MCU_ANGLE = 105,                /* MCU_ANGLE  */
  YYSYMBOL_MCU_ANGULAR_RATE = 106,         /* MCU_ANGULAR_RATE  */
  YYSYMBOL_MCU_ENERGY = 107,               /* MCU_ENERGY  */
  YYSYMBOL_MCU_EFIELD = 108,               /* MCU_EFIELD  */
  YYSYMBOL_MCU_HFIELD = 109,               /* MCU_HFIELD  */
  YYSYMBOL_MCU_FLUX = 110,                 /* MCU_FLUX  */
  YYSYMBOL_MCU_BFIELD = 111,               /* MCU_BFIELD  */
  YYSYMBOL_MCU_SLEW = 112,                 /* MCU_SLEW  */
  YYSYMBOL_MCU_NOISE = 113,                /* MCU_NOISE  */
  YYSYMBOL_MCU_CHARGE = 114,               /* MCU_CHARGE  */
  YYSYMBOL_MCUVAL_VOLT = 115,              /* MCUVAL_VOLT  */
  YYSYMBOL_MCUVAL_AMP = 116,               /* MCUVAL_AMP  */
  YYSYMBOL_MCUVAL_CAP = 117,               /* MCUVAL_CAP  */
  YYSYMBOL_MCUVAL_IND = 118,               /* MCUVAL_IND  */
  YYSYMBOL_MCUVAL_TIME = 119,              /* MCUVAL_TIME  */
  YYSYMBOL_MCUVAL_LEN = 120,               /* MCUVAL_LEN  */
  YYSYMBOL_MCUVAL_WATT = 121,              /* MCUVAL_WATT  */
  YYSYMBOL_MCUVAL_OHM = 122,               /* MCUVAL_OHM  */
  YYSYMBOL_MCUVAL_TEMP = 123,              /* MCUVAL_TEMP  */
  YYSYMBOL_MCUVAL_HZ = 124,                /* MCUVAL_HZ  */
  YYSYMBOL_MCUVAL_DB = 125,                /* MCUVAL_DB  */
  YYSYMBOL_MCUVAL_PPM = 126,               /* MCUVAL_PPM  */
  YYSYMBOL_MCUVAL_PERCENT = 127,           /* MCUVAL_PERCENT  */
  YYSYMBOL_MCUVAL_BAUD = 128,              /* MCUVAL_BAUD  */
  YYSYMBOL_MCUVAL_DATASIZE = 129,          /* MCUVAL_DATASIZE  */
  YYSYMBOL_MCUVAL_SPS = 130,               /* MCUVAL_SPS  */
  YYSYMBOL_MCUVAL_SIEMENS = 131,           /* MCUVAL_SIEMENS  */
  YYSYMBOL_MCUVAL_RESPONSIVITY = 132,      /* MCUVAL_RESPONSIVITY  */
  YYSYMBOL_MCUVAL_ANGLE = 133,             /* MCUVAL_ANGLE  */
  YYSYMBOL_MCUVAL_ANGULAR_RATE = 134,      /* MCUVAL_ANGULAR_RATE  */
  YYSYMBOL_MCUVAL_ENERGY = 135,            /* MCUVAL_ENERGY  */
  YYSYMBOL_MCUVAL_EFIELD = 136,            /* MCUVAL_EFIELD  */
  YYSYMBOL_MCUVAL_HFIELD = 137,            /* MCUVAL_HFIELD  */
  YYSYMBOL_MCUVAL_FLUX = 138,              /* MCUVAL_FLUX  */
  YYSYMBOL_MCUVAL_BFIELD = 139,            /* MCUVAL_BFIELD  */
  YYSYMBOL_MCUVAL_SLEW = 140,              /* MCUVAL_SLEW  */
  YYSYMBOL_MCUVAL_NOISE = 141,             /* MCUVAL_NOISE  */
  YYSYMBOL_MCUVAL_CHARGE = 142,            /* MCUVAL_CHARGE  */
  YYSYMBOL_MC_WS = 143,                    /* MC_WS  */
  YYSYMBOL_MC_SINGLE_COMMENT = 144,        /* MC_SINGLE_COMMENT  */
  YYSYMBOL_MC_MULTI_COMMENT = 145,         /* MC_MULTI_COMMENT  */
  YYSYMBOL_YYACCEPT = 146,                 /* $accept  */
  YYSYMBOL_start = 147,                    /* start  */
  YYSYMBOL_mc_tops = 148,                  /* mc_tops  */
  YYSYMBOL_mc_top = 149,                   /* mc_top  */
  YYSYMBOL_mc_use = 150,                   /* mc_use  */
  YYSYMBOL_mc_uri = 151,                   /* mc_uri  */
  YYSYMBOL_mc_prefix = 152,                /* mc_prefix  */
  YYSYMBOL_mc_uri_trunk = 153,             /* mc_uri_trunk  */
  YYSYMBOL_mc_levels = 154,                /* mc_levels  */
  YYSYMBOL_mc_class_name = 155,            /* mc_class_name  */
  YYSYMBOL_mc_component = 156,             /* mc_component  */
  YYSYMBOL_mc_module = 157,                /* mc_module  */
  YYSYMBOL_mc_interface = 158,             /* mc_interface  */
  YYSYMBOL_mc_enum = 159,                  /* mc_enum  */
  YYSYMBOL_mc_define = 160,                /* mc_define  */
  YYSYMBOL_mc_capability = 161,            /* mc_capability  */
  YYSYMBOL_mc_component_derivs = 162,      /* mc_component_derivs  */
  YYSYMBOL_mc_component_variant = 163,     /* mc_component_variant  */
  YYSYMBOL_mc_component_adopts = 164,      /* mc_component_adopts  */
  YYSYMBOL_mc_body = 165,                  /* mc_body  */
  YYSYMBOL_mc_clauses = 166,               /* mc_clauses  */
  YYSYMBOL_mc_clause = 167,                /* mc_clause  */
  YYSYMBOL_mc_attribute = 168,             /* mc_attribute  */
  YYSYMBOL_mc_attr_values = 169,           /* mc_attr_values  */
  YYSYMBOL_mc_attr_value = 170,            /* mc_attr_value  */
  YYSYMBOL_mc_attr_lines = 171,            /* mc_attr_lines  */
  YYSYMBOL_mc_attribute_pin = 172,         /* mc_attribute_pin  */
  YYSYMBOL_mc_pins_lines = 173,            /* mc_pins_lines  */
  YYSYMBOL_mc_pins_line = 174,             /* mc_pins_line  */
  YYSYMBOL_mc_pin_idn = 175,               /* mc_pin_idn  */
  YYSYMBOL_mc_pins_names = 176,            /* mc_pins_names  */
  YYSYMBOL_mc_pins_name = 177,             /* mc_pins_name  */
  YYSYMBOL_mc_net = 178,                   /* mc_net  */
  YYSYMBOL_mc_opds = 179,                  /* mc_opds  */
  YYSYMBOL_mc_net_port_elem = 180,         /* mc_net_port_elem  */
  YYSYMBOL_mc_net_port_elems = 181,        /* mc_net_port_elems  */
  YYSYMBOL_mc_mn_opd = 182,                /* mc_mn_opd  */
  YYSYMBOL_mc_mn_opds = 183,               /* mc_mn_opds  */
  YYSYMBOL_mc_opd = 184,                   /* mc_opd  */
  YYSYMBOL_mc_phrases = 185,               /* mc_phrases  */
  YYSYMBOL_mc_phrase = 186,                /* mc_phrase  */
  YYSYMBOL_mc_role = 187,                  /* mc_role  */
  YYSYMBOL_mc_conduit = 188,               /* mc_conduit  */
  YYSYMBOL_mc_domain = 189,                /* mc_domain  */
  YYSYMBOL_mc_rail = 190,                  /* mc_rail  */
  YYSYMBOL_mc_partition = 191,             /* mc_partition  */
  YYSYMBOL_mc_error_stmt = 192,            /* mc_error_stmt  */
  YYSYMBOL_mc_error_msg = 193,             /* mc_error_msg  */
  YYSYMBOL_mc_tattrs = 194,                /* mc_tattrs  */
  YYSYMBOL_mc_tattrs_opt = 195,            /* mc_tattrs_opt  */
  YYSYMBOL_mc_tattr = 196,                 /* mc_tattr  */
  YYSYMBOL_mc_tkey = 197,                  /* mc_tkey  */
  YYSYMBOL_mc_resword = 198,               /* mc_resword  */
  YYSYMBOL_mc_function = 199,              /* mc_function  */
  YYSYMBOL_mc_paramds = 200,               /* mc_paramds  */
  YYSYMBOL_mc_pards = 201,                 /* mc_pards  */
  YYSYMBOL_mc_pard = 202,                  /* mc_pard  */
  YYSYMBOL_mc_declare_a = 203,             /* mc_declare_a  */
  YYSYMBOL_mc_declare_a1 = 204,            /* mc_declare_a1  */
  YYSYMBOL_mc_insts = 205,                 /* mc_insts  */
  YYSYMBOL_mc_inst = 206,                  /* mc_inst  */
  YYSYMBOL_mc_iface_name = 207,            /* mc_iface_name  */
  YYSYMBOL_mc_declare_b = 208,             /* mc_declare_b  */
  YYSYMBOL_mc_params = 209,                /* mc_params  */
  YYSYMBOL_mc_param = 210,                 /* mc_param  */
  YYSYMBOL_mc_conds = 211,                 /* mc_conds  */
  YYSYMBOL_mc_conds_elifs = 212,           /* mc_conds_elifs  */
  YYSYMBOL_mc_cond_block = 213,            /* mc_cond_block  */
  YYSYMBOL_mc_expr = 214,                  /* mc_expr  */
  YYSYMBOL_mc_judge = 215,                 /* mc_judge  */
  YYSYMBOL_mc_id = 216,                    /* mc_id  */
  YYSYMBOL_mc_ida = 217,                   /* mc_ida  */
  YYSYMBOL_mc_idss = 218,                  /* mc_idss  */
  YYSYMBOL_mc_ids = 219,                   /* mc_ids  */
  YYSYMBOL_mc_idseg = 220,                 /* mc_idseg  */
  YYSYMBOL_mc_idm = 221,                   /* mc_idm  */
  YYSYMBOL_mc_idans = 222,                 /* mc_idans  */
  YYSYMBOL_mc_idan = 223,                  /* mc_idan  */
  YYSYMBOL_mc_int = 224,                   /* mc_int  */
  YYSYMBOL_mc_hex = 225,                   /* mc_hex  */
  YYSYMBOL_mc_float = 226,                 /* mc_float  */
  YYSYMBOL_mc_number = 227,                /* mc_number  */
  YYSYMBOL_mc_string = 228,                /* mc_string  */
  YYSYMBOL_mc_const = 229,                 /* mc_const  */
  YYSYMBOL_mc_nc = 230,                    /* mc_nc  */
  YYSYMBOL_mc_underscore = 231,            /* mc_underscore  */
  YYSYMBOL_mc_literal = 232,               /* mc_literal  */
  YYSYMBOL_mc_iotype = 233,                /* mc_iotype  */
  YYSYMBOL_mc_unit_value = 234,            /* mc_unit_value  */
  YYSYMBOL_mc_unit_type = 235,             /* mc_unit_type  */
  YYSYMBOL_mc_endls = 236                  /* mc_endls  */
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
#define YYLAST   4012
/* YYNTOKENS -- Number of terminals.  */
#define YYNTOKENS  146
/* YYNNTS -- Number of nonterminals.  */
#define YYNNTS  91
/* YYNRULES -- Number of rules.  */
#define YYNRULES  454
/* YYNSTATES -- Number of states.  */
#define YYNSTATES  761
/* YYMAXRHS -- Maximum number of symbols on right-hand side of rule.  */
#define YYMAXRHS 9
/* YYMAXLEFT -- Maximum number of symbols to the left of a handle
   accessed by $0, $-1, etc., in any rule.  */
#define YYMAXLEFT 0
/* YYMAXUTOK -- Last valid token kind.  */
#define YYMAXUTOK   400
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
     145
};
#if MCA_DEBUG
/* YYRLINE[YYN] -- source line where rule number YYN was defined.  */
static const yytype_int16 yyrline[] =
{
       0,   186,   186,   187,   188,   189,   190,   192,   193,   195,
     196,   197,   198,   199,   200,   201,   202,   210,   215,   229,
     234,   253,   261,   271,   277,   285,   289,   299,   300,   301,
     303,   307,   312,   317,   322,   328,   339,   355,   356,   357,
     358,   359,   360,   364,   365,   374,   385,   399,   409,   419,
     428,   434,   442,   445,   447,   449,   451,   454,   467,   468,
     469,   471,   472,   473,   474,   475,   476,   477,   478,   479,
     480,   481,   482,   485,   504,   515,   520,   527,   531,   535,
     539,   545,   546,   550,   560,   570,   583,   584,   585,   587,
     594,   602,   610,   620,   632,   641,   652,   663,   673,   678,
     683,   688,   693,   700,   704,   709,   714,   722,   727,   732,
     737,   746,   759,   760,   774,   775,   776,   777,   793,   794,
     795,   800,   801,   804,   809,   814,   820,   826,   834,   846,
     852,   858,   866,   875,   880,   887,   892,   897,   902,   903,
     904,   909,   910,   912,   913,   914,   915,   917,   918,   919,
     920,   921,   922,   923,   924,   925,   927,   928,   929,   930,
     931,   932,   933,   934,   935,   937,   938,   939,   940,   941,
     942,   943,   944,   945,   954,   955,   956,   958,   959,   960,
     961,   962,   963,   964,   965,   966,   968,   969,   970,   971,
     972,   973,   974,   975,   976,   978,   979,   980,   981,   982,
     983,   984,   985,   986,   988,   989,   990,   991,   993,   994,
     995,   996,   998,  1003,  1008,  1013,  1032,  1043,  1052,  1057,
    1064,  1072,  1082,  1092,  1101,  1110,  1118,  1126,  1134,  1144,
    1153,  1164,  1175,  1185,  1194,  1203,  1212,  1246,  1253,  1262,
    1269,  1280,  1285,  1293,  1307,  1313,  1319,  1336,  1353,  1363,
    1367,  1371,  1375,  1379,  1383,  1396,  1401,  1408,  1411,  1416,
    1421,  1432,  1436,  1440,  1452,  1452,  1452,  1452,  1452,  1452,
    1452,  1454,  1466,  1478,  1479,  1480,  1482,  1483,  1487,  1491,
    1499,  1505,  1512,  1518,  1524,  1530,  1536,  1542,  1548,  1554,
    1560,  1570,  1579,  1595,  1607,  1615,  1624,  1632,  1640,  1649,
    1654,  1661,  1666,  1676,  1677,  1678,  1685,  1693,  1703,  1704,
    1705,  1708,  1712,  1716,  1720,  1728,  1735,  1747,  1758,  1770,
    1775,  1784,  1789,  1801,  1806,  1813,  1818,  1823,  1830,  1831,
    1832,  1834,  1835,  1836,  1837,  1838,  1839,  1840,  1841,  1842,
    1843,  1844,  1845,  1846,  1851,  1852,  1853,  1855,  1856,  1857,
    1860,  1861,  1862,  1863,  1864,  1865,  1866,  1867,  1868,  1869,
    1870,  1871,  1873,  1874,  1875,  1876,  1877,  1878,  1880,  1882,
    1883,  1884,  1885,  1890,  1891,  1892,  1893,  1897,  1901,  1905,
    1911,  1912,  1913,  1914,  1915,  1916,  1917,  1918,  1925,  1926,
    1927,  1928,  1929,  1930,  1931,  1932,  1933,  1934,  1935,  1936,
    1937,  1938,  1939,  1940,  1941,  1942,  1943,  1944,  1945,  1946,
    1947,  1948,  1949,  1950,  1951,  1952,  1956,  1957,  1958,  1959,
    1960,  1961,  1962,  1963,  1964,  1965,  1966,  1967,  1968,  1969,
    1970,  1971,  1972,  1973,  1974,  1975,  1976,  1977,  1978,  1979,
    1980,  1981,  1982,  1983,  1984,  1985,  1986,  1987,  1988,  1993,
    1998,  2004,  2005,  2006,  2007
};
#endif
#define YYPACT_NINF (-622)
#define YYTABLE_NINF (-361)
/* YYPACT[STATE-NUM] -- Index in YYTABLE of the portion describing
   STATE-NUM.  */
static const yytype_int16 yypact[] =
{
    1239,  -622,    27,    57,   131,   131,   131,   131,   131,   131,
      51,  -622,  -622,    83,    63,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  1050,   108,  -622,  -622,    91,  -622,   111,
     133,   162,  1087,  -622,  -622,   128,  -622,  -622,   184,   295,
     128,   128,   200,   226,   226,   131,  -622,  1131,  -622,  -622,
      63,  -622,   241,   213,  -622,   131,   258,   131,  -622,   133,
     550,  -622,   133,   133,   133,   133,   492,   265,   305,   131,
     551,  -622,   226,   226,   131,  2295,  -622,  -622,   128,  -622,
    1181,   131,  -622,   353,  -622,   131,  -622,  -622,   374,   384,
    -622,  -622,  -622,  -622,  -622,   392,   306,    71,  3141,   288,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,   321,
     310,   170,  -622,  -622,  -622,   298,  -622,  3141,   131,   226,
     382,  -622,  -622,  -622,  -622,   428,  -622,   463,  -622,  -622,
     547,  -622,  -622,  -622,  -622,   131,   131,   131,  3141,   131,
     507,  3141,  3141,   834,  3281,  3141,   433,  -622,  -622,   133,
     464,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,    28,
    -622,  -622,  -622,  -622,  1645,   992,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,   453,   390,  -622,
    -622,  -622,  -622,  -622,  -622,   607,  3141,   498,   265,   353,
     131,  -622,  -622,  -622,   131,   551,   516,  3141,  -622,  1645,
     312,  2474,   205,   607,   458,   519,   551,   582,  1865,   131,
     735,  -622,   305,  3870,  1865,  1745,  -622,   515,  2474,   426,
    -622,  -622,   131,  -622,  -622,  -622,   551,   305,  -622,   581,
     589,   589,  2474,   226,   551,   551,   579,   583,   335,  1566,
    2080,  -622,  -622,  3281,  1467,  2616,  1359,  2153,   607,   584,
    3841,   128,    35,  -622,  2579,  3141,   133,    35,    79,  3141,
    3141,  3141,  3141,  3141,  3141,  -622,  -622,  3141,   305,  3141,
     551,    89,  3421,   159,  3141,  3141,  3141,  3141,  3141,  3141,
    -622,  -622,  3141,   305,   589,  -622,  -622,  1865,   305,  3561,
    1865,   587,   615,  3141,  3141,  3141,  3141,  3141,  3141,   305,
    -622,    25,  1645,   992,   405,  2693,   226,  -622,  -622,  -622,
     305,   408,  3141,   593,  -622,   305,  -622,   305,  -622,   508,
    -622,  1645,  2474,   172,  -622,   145,  -622,   607,   594,   643,
    -622,   637,  -622,  -622,   178,  1745,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,  -622,  -622,  -622,  -622,   472,  1865,
     353,  -622,  -622,  -622,  2295,  -622,   226,  -622,   627,   548,
    1430,  1430,   593,  3141,  3141,  -622,  1332,  2080,   502,   609,
    3141,  3141,  3141,  3141,  3141,  3141,  3141,  3141,  3141,  3281,
    3281,  -622,  -622,   522,  2579,   616,   604,   625,   226,  3141,
    -622,   222,  -622,   556,   455,  -622,   699,  1058,   485,   618,
     380,  -622,   288,  3141,  -622,   525,   520,   520,   578,   595,
     578,   595,   578,   595,   203,   578,   595,   203,   231,   187,
    -622,   231,   187,  -622,  2020,   819,   324,  -622,   699,  1058,
     485,   620,  -622,  -622,  -622,  -622,   621,  -622,  3141,   415,
    1645,  2332,   443,   288,  -622,   531,   520,   520,   578,   595,
     578,   595,   578,   595,   203,   578,   595,   203,   231,   187,
    -622,   231,   187,  -622,  2020,   819,   324,  -622,  -622,   286,
     131,  3701,   644,  -622,  1645,  2474,   607,   319,   131,   131,
     699,  1058,   485,   578,   595,   203,   578,   595,   203,   231,
     187,  -622,   231,   187,  -622,  2020,   819,   324,  -622,   225,
    -622,  -622,  -622,  -622,  -622,  -622,  1645,  2474,   607,   131,
    -622,  -622,   551,  1865,   611,  3561,  3561,  1865,   305,  -622,
     131,   467,  3141,  1745,  1745,   323,    84,  -622,   626,   647,
    -622,    81,  -622,   651,  2474,   652,   653,  3141,    95,   336,
     337,  -622,  3141,  1909,  -622,  -622,  -622,  -622,  -622,  -622,
    -622,  -622,  -622,  -622,   638,  3281,  2437,   558,  -622,  3841,
    -622,  3841,  -622,   424,    35,    35,  1865,    35,   454,  -622,
     438,  1865,  -622,  -622,  1865,  2861,   439,    35,  -622,   459,
    1865,  -622,  -622,  -622,   587,   615,   559,  -622,   235,   524,
    3561,   131,  -622,  -622,  -622,   310,  -622,   632,   567,  -622,
     131,  -622,  -622,   344,  -622,  -622,  -622,  2474,   607,  -622,
    -622,  -622,  -622,  1430,  3561,  -622,  2721,  3001,   657,  -622,
    -622,  -622,   447,  2153,  -622,  3281,  2437,  -622,  -622,  -622,
    -622,  -622,  -622,   588,   351,   590,   288,   226,   593,   355,
     359,  -622,  -622,  -622,  -622,  -622,  -622,  -622,   378,   633,
     115,   596,   226,   379,  -622,  -622,   268,  -622,   587,   615,
    1865,   662,   639,  -622,   272,   644,  -622,   263,  -622,  1645,
    2474,   461,  -622,  3001,  -622,  -622,  2153,  -622,  -622,  -622,
    -622,  -622,  -622,   611,  -622,  -622,  -622,  -622,  -622,   611,
     676,  -622,   665,   381,  3561,  1865,  -622,  3561,  3001,    97,
     277,  -622,  -622,  -622,  -622,   551,  -622,  -622,   402,   644,
    -622,  3561,  3561,   103,   606,  -622,   644,   644,  3561,   647,
     644
};
/* YYDEFACT[STATE-NUM] -- Default reduction number in state STATE-NUM.
   Performed when YYTABLE does not specify something else to do.  Zero
   means the default is an error.  */
static const yytype_int16 yydefact[] =
{
       0,    16,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   452,   451,     0,     2,     8,     9,    10,    11,    12,
      13,    14,    15,     0,     0,    21,   344,     0,    27,    19,
       0,    23,     0,    42,   345,   275,   346,   351,    44,   350,
     275,   275,     0,     0,     0,     0,     1,     0,   454,   453,
       4,    22,    17,     0,    28,     0,    25,     0,    36,     0,
       0,    33,     0,     0,     0,     0,     0,    54,     0,     0,
       0,   352,     0,     0,     0,     0,    50,    51,   275,     7,
       0,     0,    29,    20,   349,     0,    24,    37,    32,    31,
      40,    39,    41,    38,   372,   278,   129,     0,     0,     0,
     274,   382,   380,   381,   386,   387,   383,   384,   385,   125,
       0,     0,   277,   288,   289,   280,   133,     0,     0,     0,
      56,   362,    43,   353,   359,     0,   357,   360,    47,    48,
       0,    72,   363,   364,   368,     0,     0,     0,     0,     0,
     129,     0,     0,     0,     0,     0,     0,   369,   370,     0,
     125,   388,   389,   390,   391,   392,   393,   394,   395,   396,
     397,   398,   399,   400,   401,   402,   403,   404,   405,   406,
     407,   408,   409,   410,   411,   412,   413,   414,   415,     0,
      59,    61,    62,    63,     0,   257,    64,    67,    68,    69,
      70,    71,    65,   109,   141,   142,    66,   346,   123,   365,
     366,   367,   373,   374,   375,     0,     0,   376,    54,    18,
     348,    26,    35,    34,     0,     0,   130,     0,   283,   139,
       0,   138,   123,   140,     0,   123,     0,   126,   310,     0,
       0,   273,     0,     0,   310,     0,   296,   301,   286,   123,
      53,    45,     0,    52,   354,   355,     0,     0,    49,     0,
     257,   257,   246,     0,     0,     0,     0,     0,     0,     0,
       0,   379,   378,     0,   329,   330,     0,     0,   328,   111,
       0,   275,     0,    57,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   145,   143,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     146,   144,     0,     0,   258,   110,   255,   310,     0,     0,
     310,   293,   296,     0,     0,     0,     0,     0,     0,     0,
     117,   257,   114,   257,   142,     0,     0,   347,   279,   131,
       0,     0,     0,   284,   134,     0,   127,     0,   313,   129,
     371,   314,   315,     0,   309,   123,   312,   311,     0,   303,
     276,   124,   281,   282,     0,     0,   416,   417,   418,   419,
     420,   421,   422,   423,   424,   430,   431,   425,   426,   427,
     428,   429,   432,   433,   434,   435,   436,   437,   438,   439,
     440,   441,   442,   443,   444,   445,   446,   447,   290,   310,
      55,   356,   358,   361,     0,   244,     0,   247,   346,     0,
       0,     0,   218,     0,     0,   219,   329,   330,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   327,   325,   319,     0,   250,     0,   249,     0,     0,
     122,     0,   118,   351,   119,    58,   195,   201,   196,     0,
       0,   119,     0,     0,   237,     0,   225,   227,   208,   210,
     204,   206,   147,   153,   148,   156,   162,   157,   165,   171,
     166,   177,   183,   178,   186,   192,   187,   174,   199,   203,
     200,   346,   213,   212,   262,   263,   259,   261,     0,     0,
     118,     0,   365,     0,   239,     0,   226,   228,   209,   211,
     205,   207,   151,   155,   152,   160,   164,   161,   169,   173,
     170,   181,   185,   182,   190,   194,   191,   176,   256,     0,
     124,     0,    73,    76,    78,    79,    77,     0,     0,     0,
     197,   202,   198,   149,   154,   150,   158,   163,   159,   167,
     172,   168,   179,   184,   180,   188,   193,   189,   175,     0,
     107,   108,   377,    46,   132,   285,   137,   135,   136,     0,
     124,   128,     0,     0,     0,     0,     0,   310,     0,   297,
       0,     0,     0,     0,     0,     0,     0,   245,     0,   355,
      97,     0,    87,     0,   100,   351,   365,     0,     0,     0,
       0,   343,     0,   329,   331,   334,   335,   336,   337,   338,
     339,   340,   341,   332,   333,     0,     0,   321,   326,     0,
     248,     0,   272,     0,     0,     0,   310,     0,     0,   113,
       0,   310,   233,   235,   310,     0,     0,     0,   214,     0,
     310,   234,   236,   220,   294,   297,   129,    82,     0,   123,
       0,     0,   299,   300,   116,   114,   115,     0,     0,   308,
       0,   317,   316,     0,   304,   298,   450,   292,   291,   449,
     448,   302,   243,     0,     0,    83,     0,     0,     0,    84,
     241,   242,     0,     0,   320,     0,     0,   251,   252,   253,
     254,   120,   121,     0,     0,     0,     0,     0,     0,     0,
       0,   266,   264,   265,   267,   268,   269,   270,     0,     0,
     218,     0,     0,     0,   229,    80,     0,    75,   295,   298,
     310,   355,     0,   305,     0,    74,    86,    89,   102,   105,
     106,   365,   103,     0,   342,   323,     0,   322,   215,   221,
     216,   112,   238,   223,   222,   260,   271,   217,   240,   224,
       0,    81,     0,     0,     0,   310,    85,     0,     0,    93,
      91,   324,   230,   231,   232,     0,   307,   318,     0,    90,
     101,     0,     0,    94,     0,   306,    95,    92,     0,     0,
      96
};
/* YYPGOTO[NTERM-NUM].  */
static const yytype_int16 yypgoto[] =
{
    -622,  -622,   679,   138,  -622,   680,  -622,   673,  -622,    41,
    -622,  -622,  -622,  -622,  -622,  -622,   499,  -622,  -622,    31,
     315,   -64,  -499,  -581,  -530,  -622,  -622,  -383,    50,   134,
       2,   -28,  -622,   242,   185,  -622,   122,  -255,   614,   -88,
    1021,  -622,  -622,  -622,  -622,  -622,  -622,  -622,  -565,  -174,
    -295,  -622,  -622,  -622,   -10,  -622,   497,  -622,   -46,  -486,
    -185,  -520,   -34,  -226,   175,  -622,  -622,  -561,   580,  -246,
      -3,   429,   -41,   183,  -622,   -24,  -248,   484,   112,  -622,
    -622,   591,  -622,  -622,  -621,  -622,  1499,   -47,  -120,  -339,
      14
};
/* YYDEFGOTO[NTERM-NUM].  */
static const yytype_int16 yydefgoto[] =
{
       0,    13,    14,    15,    16,    29,    30,    31,    32,    35,
      17,    18,    19,    20,    21,    22,   119,   120,   243,   421,
     179,   422,   181,   512,   513,   628,   182,   571,   572,   573,
     707,   708,   183,   608,   320,   321,   430,   431,   184,   258,
     265,   186,   187,   188,   189,   190,   191,   426,   304,   305,
     306,   476,   689,   192,    67,   111,   112,   193,   194,   311,
     236,   348,   195,   343,   344,   196,   597,   423,   266,   267,
     197,    37,    83,   222,    39,   216,   125,   126,   199,   200,
     201,   202,   203,   204,   346,   116,   205,   206,   207,   388,
     424
};
/* YYTABLE[YYPACT[STATE-NUM]] -- What to do in state STATE-NUM.  If
   positive, shift that token.  If negative, reduce the rule whose
   number is the opposite.  If YYTABLE_NINF, syntax error.  */
static const yytype_int16 yytable[] =
{
      33,    36,    36,    36,    36,    36,    36,   399,   354,   508,
     220,   180,   627,   312,    23,    71,   561,   408,   578,   117,
     113,    33,   440,   262,   624,   641,   642,    33,    47,   637,
      72,    73,   114,   130,   688,   664,   712,   479,   121,    24,
     209,   291,    36,    26,    34,    94,    40,    41,    42,    43,
      44,   273,    36,   539,    36,    11,    87,   269,    25,    90,
      91,    92,    93,    36,    80,    26,    36,    36,   208,    45,
      96,    36,    27,   705,    76,    77,   395,   396,    36,    26,
      34,   509,    36,    46,   517,   227,    78,    26,    34,    94,
      11,   429,   712,    99,    36,    12,    36,    26,    34,   244,
     697,    75,   715,   128,   129,   717,    53,   652,    11,    51,
      28,    11,    36,   291,    96,    36,    26,   712,   474,   291,
     702,   109,    11,    27,    55,   751,   227,   217,   442,   331,
      12,   758,    36,    36,    36,   443,    36,    99,   655,    26,
      34,    26,   739,  -120,    54,   698,   271,   540,    12,   541,
     241,    12,   659,    26,    34,   741,   749,   475,   555,   240,
     232,    28,    12,   565,  -120,   109,   559,    26,    34,    94,
     756,   757,   324,   593,   594,   753,    57,   760,   549,   556,
     122,    75,   127,   117,   113,    79,    66,    38,    38,    38,
      38,    38,    38,   274,    96,    36,   114,   731,   230,    68,
     553,   390,   290,   234,   747,   542,   553,    36,   483,   292,
     435,    36,    36,    26,    34,   443,   313,    99,    79,    36,
     232,   391,    74,    36,   649,   650,    36,    36,    38,   231,
      36,   554,   446,    26,    34,    94,    36,   560,    84,    36,
      86,   300,   301,    36,   447,   109,   276,   486,    75,   115,
     604,   398,    36,   277,    81,   316,   317,    84,   198,   487,
      96,   428,    11,   234,    84,   319,    82,    36,   211,    36,
     704,   605,    85,   439,    36,    36,    26,    34,   118,   291,
     218,   443,   225,    99,   397,   285,   286,   471,    36,   228,
      36,   737,   695,   291,   229,    48,    26,    34,    94,    11,
     239,    38,    12,   730,   638,   752,    26,    34,   121,   444,
      69,   109,   738,   232,   553,   579,   580,    70,   249,   250,
     251,   215,   253,    96,   484,   625,   738,   329,    70,   736,
     180,   643,   233,   632,   633,    49,   226,   313,   336,    12,
     332,   603,    36,    70,   351,   623,    99,   553,    36,   663,
     673,   553,   675,   577,   577,   610,   234,   543,   127,   393,
     598,   235,   691,   332,   332,   332,   329,   127,   228,   333,
     314,   315,   553,   229,   109,   645,   316,   317,   631,   553,
     674,   210,   651,   553,   434,   679,   319,   553,   680,   441,
     616,   212,   402,   327,   693,   660,   661,   328,    26,    34,
     467,   213,   473,   703,   482,   308,   630,   553,   604,   553,
     719,   345,   349,   115,   723,   507,   352,   345,   724,   716,
     510,  -115,   612,   613,   309,    84,   214,   567,  -115,   607,
     553,   538,  -115,  -115,    26,    34,   332,   725,   729,    36,
     746,   232,   544,   604,   508,   242,   699,   550,   310,   551,
     198,   245,   332,   225,  -287,   225,   246,   198,   508,   602,
     225,   755,   621,   622,   617,   545,   332,   332,   247,  -115,
    -115,  -119,  -115,   472,   733,   332,   247,  -104,  -360,   226,
      36,   671,   676,  -119,   234,  -287,   272,   676,  -104,  -104,
     345,   270,  -119,   345,   662,   678,   690,   754,   123,   124,
      26,    34,    94,   677,   714,   636,   562,    36,   692,   748,
    -104,   307,   576,   576,   325,    36,    36,   334,  -104,   563,
     564,    95,   254,   215,   563,   564,   646,    96,  -104,   255,
     552,   330,    26,    34,   335,    69,    36,   316,   317,   232,
      97,   256,    70,   257,   237,   391,    36,   319,    98,    36,
      99,   100,   419,   420,   121,    88,    89,    36,   309,    26,
      34,   581,   101,   102,   103,   104,   105,   106,   107,   108,
     248,   569,   345,   389,   215,   210,   246,   198,   109,  -359,
     274,   255,   234,   611,  -351,   656,   595,   596,   229,   620,
     701,   275,   656,   276,   229,   246,    36,   337,    36,   694,
     277,    36,    36,   394,    36,   291,   577,   198,   289,   577,
     290,   718,   332,   720,    36,   518,   604,   292,   604,   727,
     313,   278,   665,   666,   604,   225,    36,   237,    36,   759,
     283,   284,   285,   286,   246,   400,   228,    36,   293,   401,
     288,   229,   696,   519,   123,    26,    34,   298,   299,   300,
     301,   237,   557,   314,   315,   123,   549,   303,   558,   316,
     317,   568,   599,   600,   127,   582,   225,   318,   237,   319,
     644,   601,   630,    36,   640,   124,   606,   391,   614,   615,
     110,   654,   653,   123,   124,   657,   -99,   -98,   419,   576,
     700,   713,   726,    36,   629,   742,   734,   735,   745,   309,
     743,   433,    50,    56,    52,   744,   706,   326,   722,   566,
     750,   658,   219,   224,   276,   740,   441,   441,   656,   441,
     477,   277,   225,   728,   634,   619,   672,   350,   639,   441,
     392,     0,   349,     0,   261,     0,   345,     0,     0,     0,
     345,     0,    36,    26,    34,    94,     0,     0,     0,     0,
       0,   283,   284,   285,   286,   219,   259,   228,   264,   219,
       0,   288,   229,     0,    95,   576,     0,     0,   576,   711,
      96,     0,     0,     0,   237,     0,     0,     0,     0,   198,
     237,     0,   225,    97,   225,     0,     0,   225,   225,   345,
     225,    98,     0,    99,   345,     0,     0,   345,     0,     0,
     225,     0,     0,   345,     0,   101,   102,   103,   104,   105,
     106,   107,   108,     0,     0,     0,     0,     0,     0,     0,
     322,   109,     0,   349,     0,   711,     0,     0,     0,   575,
     575,   219,   289,     0,   290,     0,     0,   121,   132,   133,
       0,   292,   341,     0,   110,     0,   198,     0,   341,   198,
     711,     0,     0,     0,     0,     0,     0,   127,     0,   225,
       0,     0,   293,   294,   295,   296,   297,     0,     0,     0,
       0,   298,   299,   300,   301,     0,     0,   406,     0,   732,
       0,   303,     0,   345,   425,     0,   432,     0,     0,   436,
       0,   432,   445,   448,   450,   452,   455,   458,   461,   198,
       0,   464,     0,   468,     0,     0,   480,   485,   488,   490,
     492,   495,   498,   501,     0,     0,   504,     0,   345,     0,
       0,   341,     0,   514,   341,     0,     0,   520,   523,   526,
     529,   532,   535,     0,     0,     0,     0,     0,     0,   237,
       0,     0,     0,     0,     0,     0,   546,   237,   237,   151,
     152,   153,   154,   155,   156,   157,   158,   159,   160,   161,
     162,   163,   164,   165,   166,   167,   168,   169,   170,   171,
     172,   173,   174,   175,   176,   177,   178,     0,     0,     0,
       0,   124,     0,     0,     0,     0,     0,     0,     0,   237,
     584,   585,   586,   587,   588,   589,   590,   591,   592,     0,
       0,     0,     0,   341,     0,   289,   575,   290,   291,     0,
       0,     0,     0,     0,   292,     0,     0,   219,   219,     0,
       0,     0,     0,     0,   583,   583,   583,   583,   583,   583,
     583,   583,   583,   264,   264,   293,   294,   295,   296,   297,
       0,     0,     0,   219,   298,   299,   300,   301,     0,     0,
      -6,     1,   302,     0,   303,     0,   609,   219,   237,     0,
     237,     2,     3,     0,     0,     0,     0,     0,     4,     5,
       6,     7,     0,   290,     8,     9,    10,    48,     0,     0,
     292,     0,   575,     0,     0,   575,     0,   -30,    58,     0,
       0,     0,   219,     0,     0,     0,   185,   609,     0,     0,
     -30,   -30,    59,    60,    61,     0,     0,     0,     0,     0,
     298,   299,   300,   301,   -30,     0,     0,    49,     0,   221,
     303,     0,     0,     0,     0,   219,     0,     0,     0,     0,
       0,    -3,     1,    62,    63,     0,     0,     0,   238,    64,
      65,     0,     2,     3,     0,     0,     0,     0,     0,     4,
       5,     6,     7,   635,   -30,     8,     9,    10,    48,   252,
       0,     0,   221,   260,     0,     0,   221,   341,     0,   514,
     514,   341,     0,     0,   124,     0,     0,     0,     0,     0,
       0,    -5,     1,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     2,     3,     0,     0,   219,     0,    49,     4,
       5,     6,     7,     0,     0,     8,     9,    10,    48,   264,
       0,     0,     0,   667,     0,   669,     0,     0,   432,   432,
     341,   432,     0,     0,     0,   341,     0,   323,   341,   514,
       0,   432,     0,     0,   341,     0,     0,     0,   221,     0,
       1,     0,     0,     0,   514,     0,     0,     0,    49,   342,
       2,     3,     0,     0,     0,   342,     0,     4,     5,     6,
       7,     0,     0,     8,     9,    10,    11,     0,   514,     0,
       0,   709,     0,     0,     0,     0,     0,     0,     0,   264,
       0,     0,     0,     0,   407,     0,     0,     0,   185,     0,
     721,     0,     0,     0,     0,   185,   437,     0,     0,     0,
     449,   451,   453,   456,   459,   462,    12,     0,   465,     0,
     469,     0,     0,   481,   341,   489,   491,   493,   496,   499,
     502,     0,     0,   505,     0,     0,     0,   709,   342,     0,
     515,   342,     0,     0,   521,   524,   527,   530,   533,   536,
       0,     0,     0,     0,     0,   275,     0,   276,   514,   341,
       0,   514,   709,   547,   277,     0,     0,     0,     0,     0,
     403,     0,     0,     0,     0,   514,   514,     0,     0,     0,
       0,     0,   514,     0,     0,   278,   279,   280,   281,   282,
       0,     0,     0,     0,   283,   284,   285,   286,     0,     0,
     228,   334,   287,     0,   288,   229,   410,   411,   412,   413,
     414,   415,     0,   409,     0,     0,     0,   416,   417,     0,
     342,     0,     0,     0,     0,   185,     0,     0,     0,     0,
     418,   574,   574,     0,   221,   221,     0,     0,     0,     0,
       0,   570,     0,   121,   132,   133,     0,   134,    26,    34,
      94,     0,     0,     0,     0,   185,     0,     0,     0,     0,
     221,     0,     0,     0,     0,     0,     0,   -88,     0,     0,
       0,     0,     0,     0,   221,    96,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     275,     0,   276,     0,     0,     0,   141,   -88,   142,   277,
       0,   143,     0,     0,     0,     0,     0,   -88,     0,   221,
     101,   102,   103,   104,   105,   106,   107,   108,   147,   148,
     278,   279,   280,   281,   282,     0,   150,     0,     0,   283,
     284,   285,   286,     0,     0,   228,     0,   287,     0,   288,
     229,     0,   221,     0,     0,     0,     0,     0,   409,     0,
       0,     0,     0,     0,     0,   151,   152,   153,   154,   155,
     156,   157,   158,   159,   160,   161,   162,   163,   164,   165,
     166,   167,   168,   169,   170,   171,   172,   173,   174,   175,
     176,   177,   178,     0,   342,     0,   515,   515,   342,   275,
       0,   276,     0,   647,     0,     0,     0,     0,   277,     0,
       0,     0,     0,     0,   403,     0,     0,   223,   574,     0,
       0,     0,     0,   221,     0,     0,     0,     0,     0,   278,
     279,   280,   281,   282,     0,     0,     0,   185,   283,   284,
     285,   286,     0,     0,   228,   334,   287,   342,   288,   229,
       0,     0,   342,     0,     0,   342,   515,     0,     0,     0,
     223,   342,     0,   268,   223,     0,     0,     0,     0,     0,
       0,   515,     0,     0,     0,     0,     0,     0,   275,     0,
     276,     0,     0,     0,     0,     0,     0,   277,     0,     0,
       0,     0,     0,     0,   574,   515,     0,   574,   710,     0,
       0,     0,     0,     0,   185,     0,     0,   185,   278,   279,
     280,   281,   282,     0,     0,     0,     0,   283,   284,   285,
     286,     0,     0,   228,     0,   287,     0,   288,   229,     0,
       0,     0,     0,     0,     0,     0,   223,     0,     0,     0,
       0,   342,     0,     0,     0,     0,     0,   347,     0,     0,
       0,     0,   353,   347,   710,     0,     0,   185,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   515,   342,     0,   515,   710,
       0,     0,   268,     0,     0,     0,     0,     0,     0,   427,
       0,     0,   515,   515,   438,     0,     0,     0,     0,   515,
     454,   457,   460,   463,     0,     0,   466,     0,   470,     0,
       0,     0,     0,     0,     0,   494,   497,   500,   503,     0,
       0,   506,     0,   355,     0,     0,   347,     0,   516,   347,
       0,     0,   522,   525,   528,   531,   534,   537,     0,     0,
       0,     0,     0,     0,     0,     0,   356,   357,   358,   359,
       0,   548,   360,   361,   362,   363,   364,   365,   366,   367,
     368,   369,   370,   371,   372,   373,   374,   375,   376,   377,
     378,   379,   380,   381,   382,   383,   384,   385,   386,   387,
       0,     0,     0,     0,     0,     0,     0,     0,   121,   132,
     133,     0,   134,    26,    34,    94,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   347,     0,
       0,     0,     0,     0,   338,     0,     0,     0,     0,     0,
     339,     0,   223,   223,     0,     0,     0,     0,     0,   268,
     268,   268,   268,   268,   268,   268,   268,   268,   268,   268,
       0,   141,   275,   142,   276,     0,   143,     0,   223,     0,
       0,   277,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   223,   147,   148,   340,     0,     0,     0,     0,
       0,   150,   278,   279,   280,   281,   282,     0,     0,     0,
       0,   283,   284,   285,   286,     0,     0,   228,     0,   287,
       0,   288,   229,     0,     0,     0,     0,   223,     0,     0,
     151,   152,   153,   154,   155,   156,   157,   158,   159,   160,
     161,   162,   163,   164,   165,   166,   167,   168,   169,   170,
     171,   172,   173,   174,   175,   176,   177,   178,     0,     0,
     223,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   275,     0,   276,     0,     0,     0,     0,
       0,     0,   277,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   347,     0,   516,   516,   347,     0,     0,     0,
       0,   648,     0,   278,   279,   280,   281,   282,     0,     0,
       0,     0,   283,   284,   285,   286,     0,     0,   228,     0,
       0,   223,   288,   229,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   289,   268,   290,     0,     0,   668,     0,
     670,     0,   292,     0,     0,   347,     0,     0,   404,     0,
     347,     0,     0,   347,   516,     0,     0,     0,     0,   347,
       0,     0,     0,   293,   294,   295,   296,   297,     0,   516,
       0,     0,   298,   299,   300,   301,     0,     0,     0,   405,
     302,     0,   303,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   516,   131,     0,   121,   132,   133,     0,
     134,    26,    34,    94,   268,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    75,     0,     0,     0,     0,
      11,     0,   135,   136,   137,   138,   139,     0,   140,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   347,
       0,     0,     0,   419,   420,     0,     0,     0,     0,   141,
       0,   142,     0,     0,   143,     0,     0,     0,     0,   144,
      12,   145,   146,   101,   102,   103,   104,   105,   106,   107,
     108,   147,   148,   516,   347,     0,   516,     0,   149,   150,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     516,   516,     0,     0,     0,     0,     0,   516,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   151,   152,
     153,   154,   155,   156,   157,   158,   159,   160,   161,   162,
     163,   164,   165,   166,   167,   168,   169,   170,   171,   172,
     173,   174,   175,   176,   177,   178,   131,     0,   121,   132,
     133,     0,   134,    26,    34,    94,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   -60,     0,
       0,     0,   -60,     0,   135,   136,   137,   138,   139,     0,
     140,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   289,     0,   290,     0,     0,
       0,   141,     0,   142,   292,   618,   143,     0,     0,     0,
       0,   144,   -60,   145,   146,   101,   102,   103,   104,   105,
     106,   107,   108,   147,   148,   293,   294,   295,   296,   297,
     149,   150,     0,     0,   298,   299,   300,   301,     0,     0,
       0,     0,   302,     0,   303,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     151,   152,   153,   154,   155,   156,   157,   158,   159,   160,
     161,   162,   163,   164,   165,   166,   167,   168,   169,   170,
     171,   172,   173,   174,   175,   176,   177,   178,   131,     0,
     121,   132,   133,     0,   134,    26,    34,    94,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    75,
       0,     0,     0,     0,    11,     0,   135,   136,   137,   138,
     139,     0,   140,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   289,     0,   290,
       0,     0,     0,   141,     0,   142,   292,     0,   143,     0,
       0,     0,     0,   144,    12,   145,   146,   101,   102,   103,
     104,   105,   106,   107,   108,   147,   148,   293,   294,   295,
     296,   297,   149,   150,     0,     0,   298,   299,   300,   301,
       0,     0,     0,     0,   302,     0,   303,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   151,   152,   153,   154,   155,   156,   157,   158,
     159,   160,   161,   162,   163,   164,   165,   166,   167,   168,
     169,   170,   171,   172,   173,   174,   175,   176,   177,   178,
     131,     0,   121,   132,   133,     0,   134,    26,    34,    94,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    48,     0,   135,   136,
     137,   138,   139,     0,   140,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   289,
       0,   290,     0,     0,     0,   141,     0,   142,   292,     0,
     143,     0,     0,     0,     0,   144,    49,   145,   146,   101,
     102,   103,   104,   105,   106,   107,   108,   147,   148,   293,
     294,   295,   296,   297,   149,   150,     0,     0,   298,   299,
     300,   301,     0,     0,     0,     0,   302,     0,   303,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   151,   152,   153,   154,   155,   156,
     157,   158,   159,   160,   161,   162,   163,   164,   165,   166,
     167,   168,   169,   170,   171,   172,   173,   174,   175,   176,
     177,   178,   570,     0,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    48,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   141,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,    49,     0,
       0,   101,   102,   103,   104,   105,   106,   107,   108,   147,
     148,     0,     0,     0,     0,     0,     0,   150,   151,   152,
     153,   154,   155,   156,   157,   158,   159,   160,   161,   162,
     163,   164,   165,   166,   167,   168,   169,   170,   171,   172,
     173,   174,   175,   176,   177,   178,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   511,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,   681,   682,   683,   684,     0,   685,   686,   687,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   141,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,   340,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   141,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   141,     0,   263,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   478,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,    96,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   511,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   626,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   141,     0,   142,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   121,   132,   133,     0,   134,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   121,   132,   133,    96,   134,    26,    34,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    99,
       0,     0,   143,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   109,     0,     0,
       0,   143,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   147,   148,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,     0,   151,   152,   153,   154,   155,
     156,   157,   158,   159,   160,   161,   162,   163,   164,   165,
     166,   167,   168,   169,   170,   171,   172,   173,   174,   175,
     176,   177,   178
};
static const yytype_int16 yycheck[] =
{
       3,     4,     5,     6,     7,     8,     9,   255,   234,   304,
      98,    75,   511,   198,     0,    39,   355,   263,   401,    66,
      66,    24,   277,   143,   510,   555,   556,    30,    14,   549,
      40,    41,    66,    74,   615,   596,   657,   292,     3,    12,
      81,    16,    45,     8,     9,    10,     5,     6,     7,     8,
       9,    23,    55,    28,    57,    27,    59,   145,     1,    62,
      63,    64,    65,    66,    50,     8,    69,    70,    78,    18,
      35,    74,    15,   654,    43,    44,   250,   251,    81,     8,
       9,   307,    85,     0,   310,   109,    45,     8,     9,    10,
      27,    56,   713,    58,    97,    67,    99,     8,     9,   123,
     630,    22,   663,    72,    73,   666,    15,    23,    27,     1,
      53,    27,   115,    16,    35,   118,     8,   738,    29,    16,
     640,    86,    27,    15,    13,    28,   150,    56,    49,   217,
      67,    28,   135,   136,   137,    56,   139,    58,    57,     8,
       9,     8,   707,    28,    53,   631,   149,   321,    67,   323,
     119,    67,    57,     8,     9,   716,   737,    68,    13,   118,
      15,    53,    67,   389,    49,    86,   351,     8,     9,    10,
     751,   752,   206,   419,   420,   740,    14,   758,    63,    34,
      68,    22,    70,   230,   230,    47,    58,     4,     5,     6,
       7,     8,     9,   179,    35,   198,   230,   696,    28,    15,
      28,   242,    15,    58,   734,   325,    28,   210,    49,    22,
     274,   214,   215,     8,     9,    56,    13,    58,    80,   222,
      15,   245,    22,   226,   563,   564,   229,   230,    45,    59,
     233,    59,   278,     8,     9,    10,   239,    59,    55,   242,
      57,    54,    55,   246,   278,    86,    15,   293,    22,    66,
      28,   254,   255,    22,    13,    52,    53,    74,    75,   293,
      35,   271,    27,    58,    81,    62,    53,   270,    85,   272,
     653,    49,    14,   276,   277,   278,     8,     9,    13,    16,
      97,    56,    99,    58,   253,    54,    55,   290,   291,    58,
     293,    28,    57,    16,    63,    27,     8,     9,    10,    27,
     117,   118,    67,    35,   552,    28,     8,     9,     3,   278,
      15,    86,    49,    15,    28,   403,   404,    22,   135,   136,
     137,    15,   139,    35,   293,   510,    49,   215,    22,    57,
     394,   557,    34,   518,   519,    67,    15,    13,   226,    67,
      28,   429,   345,    22,   232,    59,    58,    28,   351,   595,
     605,    28,   607,   400,   401,   443,    58,   326,   246,   247,
     424,    63,   617,    28,    28,    28,   254,   255,    58,    57,
      46,    47,    28,    63,    86,   560,    52,    53,    59,    28,
     606,    28,    59,    28,   272,   611,    62,    28,   614,   277,
     478,    17,    57,   210,   620,    59,    59,   214,     8,     9,
     288,    17,   290,    59,   292,    15,    28,    28,    28,    28,
      59,   228,   229,   230,    59,   303,   233,   234,    59,   665,
     308,    16,   446,   447,    34,   242,    34,   396,    23,    49,
      28,   319,    27,    28,     8,     9,    28,    59,    59,   442,
      59,    15,   330,    28,   739,    63,   631,   335,    58,   337,
     267,    23,    28,   270,    28,   272,    28,   274,   753,   428,
     277,    59,   486,   487,    49,    57,    28,    28,    13,    64,
      65,    28,    67,   290,   700,    28,    13,    16,    23,    15,
     483,    57,    28,    28,    58,    59,    22,    28,    27,    28,
     307,    58,    49,   310,   582,    57,    57,   745,    69,    70,
       8,     9,    10,    49,    57,   539,    34,   510,    49,   735,
      49,    58,   400,   401,    16,   518,   519,    59,    57,    52,
      53,    29,    15,    15,    52,    53,    59,    35,    67,    22,
      22,    15,     8,     9,    15,    15,   539,    52,    53,    15,
      48,    34,    22,    36,   115,   569,   549,    62,    56,   552,
      58,    59,    50,    51,     3,     5,     6,   560,    34,     8,
       9,    59,    70,    71,    72,    73,    74,    75,    76,    77,
      23,    23,   389,    58,    15,    28,    28,   394,    86,    23,
     566,    22,    58,    58,    28,   571,    64,    65,    63,    58,
      23,    13,   578,    15,    63,    28,   599,    15,   601,   623,
      22,   604,   605,    22,   607,    16,   653,   424,    13,   656,
      15,    23,    28,    23,   617,    28,    28,    22,    28,    23,
      13,    43,    64,    65,    28,   442,   629,   198,   631,    23,
      52,    53,    54,    55,    28,    56,    58,   640,    43,    56,
      62,    63,   628,    28,   215,     8,     9,    52,    53,    54,
      55,   222,    58,    46,    47,   226,    63,    62,    15,    52,
      53,    34,    46,    59,   552,    56,   483,    60,   239,    62,
     558,    46,    28,   676,    63,   246,    58,   701,    58,    58,
      66,    34,    56,   254,   255,    34,    34,    34,    50,   577,
      58,    34,    59,   696,   511,   719,    34,    58,    22,    34,
     724,   272,    23,    30,    24,   729,   656,   208,   677,   394,
     738,   577,    98,    99,    15,   713,   604,   605,   704,   607,
     291,    22,   539,   692,   539,   483,   604,   230,   553,   617,
     246,    -1,   549,    -1,   143,    -1,   553,    -1,    -1,    -1,
     557,    -1,   745,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    52,    53,    54,    55,   141,   142,    58,   144,   145,
      -1,    62,    63,    -1,    29,   653,    -1,    -1,   656,   657,
      35,    -1,    -1,    -1,   345,    -1,    -1,    -1,    -1,   596,
     351,    -1,   599,    48,   601,    -1,    -1,   604,   605,   606,
     607,    56,    -1,    58,   611,    -1,    -1,   614,    -1,    -1,
     617,    -1,    -1,   620,    -1,    70,    71,    72,    73,    74,
      75,    76,    77,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     206,    86,    -1,   640,    -1,   713,    -1,    -1,    -1,   400,
     401,   217,    13,    -1,    15,    -1,    -1,     3,     4,     5,
      -1,    22,   228,    -1,   230,    -1,   663,    -1,   234,   666,
     738,    -1,    -1,    -1,    -1,    -1,    -1,   745,    -1,   676,
      -1,    -1,    43,    44,    45,    46,    47,    -1,    -1,    -1,
      -1,    52,    53,    54,    55,    -1,    -1,   263,    -1,   696,
      -1,    62,    -1,   700,   270,    -1,   272,    -1,    -1,   275,
      -1,   277,   278,   279,   280,   281,   282,   283,   284,   716,
      -1,   287,    -1,   289,    -1,    -1,   292,   293,   294,   295,
     296,   297,   298,   299,    -1,    -1,   302,    -1,   735,    -1,
      -1,   307,    -1,   309,   310,    -1,    -1,   313,   314,   315,
     316,   317,   318,    -1,    -1,    -1,    -1,    -1,    -1,   510,
      -1,    -1,    -1,    -1,    -1,    -1,   332,   518,   519,   115,
     116,   117,   118,   119,   120,   121,   122,   123,   124,   125,
     126,   127,   128,   129,   130,   131,   132,   133,   134,   135,
     136,   137,   138,   139,   140,   141,   142,    -1,    -1,    -1,
      -1,   552,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   560,
     410,   411,   412,   413,   414,   415,   416,   417,   418,    -1,
      -1,    -1,    -1,   389,    -1,    13,   577,    15,    16,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    -1,   403,   404,    -1,
      -1,    -1,    -1,    -1,   410,   411,   412,   413,   414,   415,
     416,   417,   418,   419,   420,    43,    44,    45,    46,    47,
      -1,    -1,    -1,   429,    52,    53,    54,    55,    -1,    -1,
       0,     1,    60,    -1,    62,    -1,   442,   443,   629,    -1,
     631,    11,    12,    -1,    -1,    -1,    -1,    -1,    18,    19,
      20,    21,    -1,    15,    24,    25,    26,    27,    -1,    -1,
      22,    -1,   653,    -1,    -1,   656,    -1,     0,     1,    -1,
      -1,    -1,   478,    -1,    -1,    -1,    75,   483,    -1,    -1,
      13,    14,    15,    16,    17,    -1,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,    27,    -1,    -1,    67,    -1,    98,
      62,    -1,    -1,    -1,    -1,   511,    -1,    -1,    -1,    -1,
      -1,     0,     1,    46,    47,    -1,    -1,    -1,   117,    52,
      53,    -1,    11,    12,    -1,    -1,    -1,    -1,    -1,    18,
      19,    20,    21,   539,    67,    24,    25,    26,    27,   138,
      -1,    -1,   141,   142,    -1,    -1,   145,   553,    -1,   555,
     556,   557,    -1,    -1,   745,    -1,    -1,    -1,    -1,    -1,
      -1,     0,     1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    11,    12,    -1,    -1,   582,    -1,    67,    18,
      19,    20,    21,    -1,    -1,    24,    25,    26,    27,   595,
      -1,    -1,    -1,   599,    -1,   601,    -1,    -1,   604,   605,
     606,   607,    -1,    -1,    -1,   611,    -1,   206,   614,   615,
      -1,   617,    -1,    -1,   620,    -1,    -1,    -1,   217,    -1,
       1,    -1,    -1,    -1,   630,    -1,    -1,    -1,    67,   228,
      11,    12,    -1,    -1,    -1,   234,    -1,    18,    19,    20,
      21,    -1,    -1,    24,    25,    26,    27,    -1,   654,    -1,
      -1,   657,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   665,
      -1,    -1,    -1,    -1,   263,    -1,    -1,    -1,   267,    -1,
     676,    -1,    -1,    -1,    -1,   274,   275,    -1,    -1,    -1,
     279,   280,   281,   282,   283,   284,    67,    -1,   287,    -1,
     289,    -1,    -1,   292,   700,   294,   295,   296,   297,   298,
     299,    -1,    -1,   302,    -1,    -1,    -1,   713,   307,    -1,
     309,   310,    -1,    -1,   313,   314,   315,   316,   317,   318,
      -1,    -1,    -1,    -1,    -1,    13,    -1,    15,   734,   735,
      -1,   737,   738,   332,    22,    -1,    -1,    -1,    -1,    -1,
      28,    -1,    -1,    -1,    -1,   751,   752,    -1,    -1,    -1,
      -1,    -1,   758,    -1,    -1,    43,    44,    45,    46,    47,
      -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,    -1,
      58,    59,    60,    -1,    62,    63,    37,    38,    39,    40,
      41,    42,    -1,    71,    -1,    -1,    -1,    48,    49,    -1,
     389,    -1,    -1,    -1,    -1,   394,    -1,    -1,    -1,    -1,
      61,   400,   401,    -1,   403,   404,    -1,    -1,    -1,    -1,
      -1,     1,    -1,     3,     4,     5,    -1,     7,     8,     9,
      10,    -1,    -1,    -1,    -1,   424,    -1,    -1,    -1,    -1,
     429,    -1,    -1,    -1,    -1,    -1,    -1,    27,    -1,    -1,
      -1,    -1,    -1,    -1,   443,    35,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      13,    -1,    15,    -1,    -1,    -1,    56,    57,    58,    22,
      -1,    61,    -1,    -1,    -1,    -1,    -1,    67,    -1,   478,
      70,    71,    72,    73,    74,    75,    76,    77,    78,    79,
      43,    44,    45,    46,    47,    -1,    86,    -1,    -1,    52,
      53,    54,    55,    -1,    -1,    58,    -1,    60,    -1,    62,
      63,    -1,   511,    -1,    -1,    -1,    -1,    -1,    71,    -1,
      -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,    -1,   553,    -1,   555,   556,   557,    13,
      -1,    15,    -1,   562,    -1,    -1,    -1,    -1,    22,    -1,
      -1,    -1,    -1,    -1,    28,    -1,    -1,    98,   577,    -1,
      -1,    -1,    -1,   582,    -1,    -1,    -1,    -1,    -1,    43,
      44,    45,    46,    47,    -1,    -1,    -1,   596,    52,    53,
      54,    55,    -1,    -1,    58,    59,    60,   606,    62,    63,
      -1,    -1,   611,    -1,    -1,   614,   615,    -1,    -1,    -1,
     141,   620,    -1,   144,   145,    -1,    -1,    -1,    -1,    -1,
      -1,   630,    -1,    -1,    -1,    -1,    -1,    -1,    13,    -1,
      15,    -1,    -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,
      -1,    -1,    -1,    -1,   653,   654,    -1,   656,   657,    -1,
      -1,    -1,    -1,    -1,   663,    -1,    -1,   666,    43,    44,
      45,    46,    47,    -1,    -1,    -1,    -1,    52,    53,    54,
      55,    -1,    -1,    58,    -1,    60,    -1,    62,    63,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   217,    -1,    -1,    -1,
      -1,   700,    -1,    -1,    -1,    -1,    -1,   228,    -1,    -1,
      -1,    -1,   233,   234,   713,    -1,    -1,   716,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   734,   735,    -1,   737,   738,
      -1,    -1,   263,    -1,    -1,    -1,    -1,    -1,    -1,   270,
      -1,    -1,   751,   752,   275,    -1,    -1,    -1,    -1,   758,
     281,   282,   283,   284,    -1,    -1,   287,    -1,   289,    -1,
      -1,    -1,    -1,    -1,    -1,   296,   297,   298,   299,    -1,
      -1,   302,    -1,    58,    -1,    -1,   307,    -1,   309,   310,
      -1,    -1,   313,   314,   315,   316,   317,   318,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    81,    82,    83,    84,
      -1,   332,    87,    88,    89,    90,    91,    92,    93,    94,
      95,    96,    97,    98,    99,   100,   101,   102,   103,   104,
     105,   106,   107,   108,   109,   110,   111,   112,   113,   114,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   389,    -1,
      -1,    -1,    -1,    -1,    29,    -1,    -1,    -1,    -1,    -1,
      35,    -1,   403,   404,    -1,    -1,    -1,    -1,    -1,   410,
     411,   412,   413,   414,   415,   416,   417,   418,   419,   420,
      -1,    56,    13,    58,    15,    -1,    61,    -1,   429,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   443,    78,    79,    80,    -1,    -1,    -1,    -1,
      -1,    86,    43,    44,    45,    46,    47,    -1,    -1,    -1,
      -1,    52,    53,    54,    55,    -1,    -1,    58,    -1,    60,
      -1,    62,    63,    -1,    -1,    -1,    -1,   478,    -1,    -1,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,    -1,    -1,
     511,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    13,    -1,    15,    -1,    -1,    -1,    -1,
      -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   553,    -1,   555,   556,   557,    -1,    -1,    -1,
      -1,   562,    -1,    43,    44,    45,    46,    47,    -1,    -1,
      -1,    -1,    52,    53,    54,    55,    -1,    -1,    58,    -1,
      -1,   582,    62,    63,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    13,   595,    15,    -1,    -1,   599,    -1,
     601,    -1,    22,    -1,    -1,   606,    -1,    -1,    28,    -1,
     611,    -1,    -1,   614,   615,    -1,    -1,    -1,    -1,   620,
      -1,    -1,    -1,    43,    44,    45,    46,    47,    -1,   630,
      -1,    -1,    52,    53,    54,    55,    -1,    -1,    -1,    59,
      60,    -1,    62,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   654,     1,    -1,     3,     4,     5,    -1,
       7,     8,     9,    10,   665,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,
      27,    -1,    29,    30,    31,    32,    33,    -1,    35,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   700,
      -1,    -1,    -1,    50,    51,    -1,    -1,    -1,    -1,    56,
      -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    66,
      67,    68,    69,    70,    71,    72,    73,    74,    75,    76,
      77,    78,    79,   734,   735,    -1,   737,    -1,    85,    86,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     751,   752,    -1,    -1,    -1,    -1,    -1,   758,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,     1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    23,    -1,
      -1,    -1,    27,    -1,    29,    30,    31,    32,    33,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    13,    -1,    15,    -1,    -1,
      -1,    56,    -1,    58,    22,    23,    61,    -1,    -1,    -1,
      -1,    66,    67,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    77,    78,    79,    43,    44,    45,    46,    47,
      85,    86,    -1,    -1,    52,    53,    54,    55,    -1,    -1,
      -1,    -1,    60,    -1,    62,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    -1,    27,    -1,    29,    30,    31,    32,
      33,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    13,    -1,    15,
      -1,    -1,    -1,    56,    -1,    58,    22,    -1,    61,    -1,
      -1,    -1,    -1,    66,    67,    68,    69,    70,    71,    72,
      73,    74,    75,    76,    77,    78,    79,    43,    44,    45,
      46,    47,    85,    86,    -1,    -1,    52,    53,    54,    55,
      -1,    -1,    -1,    -1,    60,    -1,    62,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   115,   116,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
       1,    -1,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    27,    -1,    29,    30,
      31,    32,    33,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    13,
      -1,    15,    -1,    -1,    -1,    56,    -1,    58,    22,    -1,
      61,    -1,    -1,    -1,    -1,    66,    67,    68,    69,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    79,    43,
      44,    45,    46,    47,    85,    86,    -1,    -1,    52,    53,
      54,    55,    -1,    -1,    -1,    -1,    60,    -1,    62,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     1,    -1,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    27,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    67,    -1,
      -1,    70,    71,    72,    73,    74,    75,    76,    77,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    70,    71,    72,    73,    -1,    75,    76,    77,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    80,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,     3,     4,     5,    35,     7,     8,     9,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,
      79,    -1,    -1,    -1,    -1,    -1,    -1,    86,    -1,    -1,
      -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    78,    79,
      -1,    -1,    -1,    -1,    -1,    -1,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,    -1,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142
};
/* YYSTOS[STATE-NUM] -- The symbol kind of the accessing symbol of
   state STATE-NUM.  */
static const yytype_uint8 yystos[] =
{
       0,     1,    11,    12,    18,    19,    20,    21,    24,    25,
      26,    27,    67,   147,   148,   149,   150,   156,   157,   158,
     159,   160,   161,   236,    12,     1,     8,    15,    53,   151,
     152,   153,   154,   216,     9,   155,   216,   217,   219,   220,
     155,   155,   155,   155,   155,    18,     0,   236,    27,    67,
     148,     1,   151,    15,    53,    13,   153,    14,     1,    15,
      16,    17,    46,    47,    52,    53,    58,   200,    15,    15,
      22,   221,   200,   200,    22,    22,   165,   165,   155,   149,
     236,    13,    53,   218,   219,    14,   219,   216,     5,     6,
     216,   216,   216,   216,    10,    29,    35,    48,    56,    58,
      59,    70,    71,    72,    73,    74,    75,    76,    77,    86,
     184,   201,   202,   204,   208,   219,   231,   233,    13,   162,
     163,     3,   224,   217,   217,   222,   223,   224,   165,   165,
     218,     1,     4,     5,     7,    29,    30,    31,    32,    33,
      35,    56,    58,    61,    66,    68,    69,    78,    79,    85,
      86,   115,   116,   117,   118,   119,   120,   121,   122,   123,
     124,   125,   126,   127,   128,   129,   130,   131,   132,   133,
     134,   135,   136,   137,   138,   139,   140,   141,   142,   166,
     167,   168,   172,   178,   184,   186,   187,   188,   189,   190,
     191,   192,   199,   203,   204,   208,   211,   216,   219,   224,
     225,   226,   227,   228,   229,   232,   233,   234,   200,   218,
      28,   219,    17,    17,    34,    15,   221,    56,   219,   184,
     185,   186,   219,   232,   184,   219,    15,   221,    58,    63,
      28,    59,    15,    34,    58,    63,   206,   217,   186,   219,
     155,   165,    63,   164,   221,    23,    28,    13,    23,   219,
     219,   219,   186,   219,    15,    22,    34,    36,   185,   184,
     186,   227,   234,    58,   184,   186,   214,   215,   232,   185,
      58,   216,    22,    23,   236,    13,    15,    22,    43,    44,
      45,    46,    47,    52,    53,    54,    55,    60,    62,    13,
      15,    16,    22,    43,    44,    45,    46,    47,    52,    53,
      54,    55,    60,    62,   194,   195,   196,    58,    15,    34,
      58,   205,   206,    13,    46,    47,    52,    53,    60,    62,
     180,   181,   184,   186,   208,    16,   162,   219,   219,   224,
      15,   185,    28,    57,    59,    15,   224,    15,    29,    35,
      80,   184,   186,   209,   210,   219,   230,   232,   207,   219,
     202,   224,   219,   232,   209,    58,    81,    82,    83,    84,
      87,    88,    89,    90,    91,    92,    93,    94,    95,    96,
      97,    98,    99,   100,   101,   102,   103,   104,   105,   106,
     107,   108,   109,   110,   111,   112,   113,   114,   235,    58,
     218,   221,   223,   224,    22,   195,   195,   165,   216,   222,
      56,    56,    57,    28,    28,    59,   184,   186,   215,    71,
      37,    38,    39,    40,    41,    42,    48,    49,    61,    50,
      51,   165,   167,   213,   236,   184,   193,   232,   200,    56,
     182,   183,   184,   217,   224,   167,   184,   186,   232,   216,
     183,   224,    49,    56,   165,   184,   204,   208,   184,   186,
     184,   186,   184,   186,   232,   184,   186,   232,   184,   186,
     232,   184,   186,   232,   184,   186,   232,   224,   184,   186,
     232,   216,   219,   224,    29,    68,   197,   217,    56,   183,
     184,   186,   224,    49,   165,   184,   204,   208,   184,   186,
     184,   186,   184,   186,   232,   184,   186,   232,   184,   186,
     232,   184,   186,   232,   184,   186,   232,   224,   196,   209,
     224,    56,   169,   170,   184,   186,   232,   209,    28,    28,
     184,   186,   232,   184,   186,   232,   184,   186,   232,   184,
     186,   232,   184,   186,   232,   184,   186,   232,   224,    28,
     195,   195,   234,   165,   224,    57,   184,   186,   232,    63,
     224,   224,    22,    28,    59,    13,    34,    58,    15,   206,
      59,   235,    34,    52,    53,   209,   166,   165,    34,    23,
       1,   173,   174,   175,   186,   217,   224,   233,   173,   185,
     185,    59,    56,   184,   214,   214,   214,   214,   214,   214,
     214,   214,   214,   215,   215,    64,    65,   212,   167,    46,
      59,    46,   165,   185,    28,    49,    58,    49,   179,   184,
     185,    58,   221,   221,    58,    58,   185,    49,    23,   179,
      58,   221,   221,    59,   205,   206,    35,   168,   171,   219,
      28,    59,   206,   206,   180,   184,   208,   207,   222,   210,
      63,   170,   170,   209,   224,   206,    59,   186,   232,   235,
     235,    59,    23,    56,    34,    57,   236,    34,   175,    57,
      59,    59,   185,   215,   213,    64,    65,   184,   232,   184,
     232,    57,   182,   183,   209,   183,    28,    49,    57,   209,
     209,    70,    71,    72,    73,    75,    76,    77,   169,   198,
      57,   183,    49,   209,   221,    57,   236,   170,   205,   206,
      58,    23,   207,    59,   173,   169,   174,   176,   177,   184,
     186,   224,   230,    34,    57,   213,   215,   213,    23,    59,
      23,   184,   165,    59,    59,    59,    59,    23,   165,    59,
      35,   168,   219,   209,    34,    58,    57,    28,    49,   194,
     176,   213,   221,   221,   221,    22,    59,   170,   209,   169,
     177,    28,    28,   194,   222,    59,   169,   169,    28,    23,
     169
};
/* YYR1[RULE-NUM] -- Symbol kind of the left-hand side of rule RULE-NUM.  */
static const yytype_uint8 yyr1[] =
{
       0,   146,   147,   147,   147,   147,   147,   148,   148,   149,
     149,   149,   149,   149,   149,   149,   149,   150,   150,   150,
     150,   150,   150,   151,   151,   151,   151,   152,   152,   152,
     153,   153,   153,   153,   153,   153,   153,   154,   154,   154,
     154,   154,   154,   155,   155,   156,   156,   157,   158,   159,
     160,   161,   162,   163,   163,   164,   164,   165,   166,   166,
     166,   167,   167,   167,   167,   167,   167,   167,   167,   167,
     167,   167,   167,   168,   168,   169,   169,   170,   170,   170,
     170,   171,   171,   172,   172,   172,   173,   173,   173,   174,
     174,   174,   174,   174,   174,   174,   174,   174,   175,   175,
     175,   176,   176,   177,   177,   177,   177,   178,   178,   178,
     178,   178,   179,   179,   180,   180,   181,   181,   182,   182,
     182,   183,   183,   184,   184,   184,   184,   184,   184,   184,
     184,   184,   184,   184,   184,   185,   185,   185,   185,   185,
     185,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   186,   186,   186,   186,   186,   186,   186,
     186,   186,   186,   187,   188,   189,   190,   191,   192,   193,
     193,   193,   193,   193,   193,   194,   194,   195,   195,   196,
     196,   197,   197,   197,   198,   198,   198,   198,   198,   198,
     198,   196,   199,   200,   200,   200,   201,   201,   202,   202,
     202,   202,   202,   202,   202,   202,   202,   202,   202,   202,
     202,   202,   202,   203,   203,   203,   204,   204,   204,   205,
     205,   206,   206,   207,   207,   208,   208,   208,   209,   209,
     209,   210,   210,   210,   210,   210,   210,   210,   210,   211,
     211,   211,   211,   212,   212,   213,   213,   213,   214,   214,
     214,   215,   215,   215,   215,   215,   215,   215,   215,   215,
     215,   215,   215,   215,   216,   217,   217,   218,   218,   218,
     219,   220,   220,   221,   221,   221,   221,   222,   222,   223,
     223,   223,   224,   225,   226,   227,   227,   227,   228,   229,
     229,   230,   231,   232,   232,   232,   232,   232,   232,   232,
     233,   233,   233,   233,   233,   233,   233,   233,   234,   234,
     234,   234,   234,   234,   234,   234,   234,   234,   234,   234,
     234,   234,   234,   234,   234,   234,   234,   234,   234,   234,
     234,   234,   234,   234,   234,   234,   235,   235,   235,   235,
     235,   235,   235,   235,   235,   235,   235,   235,   235,   235,
     235,   235,   235,   235,   235,   235,   235,   235,   235,   235,
     235,   235,   235,   235,   235,   235,   235,   235,   235,   235,
     235,   236,   236,   236,   236
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
       1,     1,     1,     3,     6,     3,     1,     1,     1,     1,
       3,     3,     1,     5,     5,     7,     3,     1,     0,     3,
       5,     4,     6,     4,     5,     6,     7,     1,     1,     1,
       1,     3,     1,     1,     1,     1,     1,     3,     3,     1,
       2,     2,     3,     1,     1,     1,     3,     1,     1,     1,
       3,     3,     1,     1,     3,     1,     2,     3,     4,     1,
       2,     3,     4,     1,     3,     3,     3,     3,     1,     1,
       1,     1,     1,     2,     2,     2,     2,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     4,     6,     6,     6,     3,     3,
       4,     6,     6,     6,     6,     3,     3,     3,     3,     5,
       7,     7,     7,     4,     4,     4,     4,     3,     6,     3,
       6,     5,     5,     5,     3,     4,     2,     3,     4,     1,
       1,     3,     3,     3,     3,     1,     2,     0,     1,     2,
       5,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     5,     4,     3,     2,     0,     3,     1,     1,     3,
       1,     3,     3,     2,     3,     4,     2,     2,     1,     1,
       3,     5,     5,     2,     4,     5,     2,     4,     5,     3,
       3,     1,     4,     1,     3,     6,     9,     8,     3,     1,
       0,     1,     1,     1,     1,     1,     3,     3,     6,     3,
       5,     4,     6,     3,     4,     1,     2,     1,     1,     1,
       1,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     5,     3,     1,     1,     1,     3,     2,     1,
       1,     1,     2,     2,     3,     3,     4,     1,     3,     1,
       1,     3,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     3,     2,     2,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     3,     3,
       3,     1,     1,     2,     2
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
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     2,     1,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     1,     0,     2,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0
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
       0,     0,     0,     0,     0
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
       0,     0,     0,     0,     0
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
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    45,     0,
      47,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    49,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    17,    19,     0,     0,     0,     0,     0,
      21,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    23,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       1,     0,     0,     0,     0,     0,     0,     3,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     5,     0,     0,     0,
       0,     7,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    11,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    33,     0,     0,     0,     0,     0,     0,    35,     0,
       0,     0,    37,     0,     0,     0,     0,     0,     0,     0,
       0,    27,     0,     0,     0,     0,     0,     0,    13,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    39,
      41,     0,    43,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    57,    29,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     9,     0,     0,    15,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    65,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    25,     0,     0,     0,     0,     0,     0,
       0,     0,    67,     0,    55,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    51,    53,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    59,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    61,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,    63,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    31,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0
};
/* YYCONFL[I] -- lists of conflicting rule numbers, each terminated by
   0, pointed into by YYCONFLP.  */
static const short yyconfl[] =
{
       0,   350,     0,   350,     0,   123,     0,   123,     0,   346,
       0,   123,     0,   123,     0,   376,     0,   123,     0,   123,
       0,   123,     0,   123,     0,   301,     0,   123,     0,   123,
       0,   330,     0,   142,     0,   142,     0,   142,     0,   142,
       0,   142,     0,   142,     0,   123,     0,   123,     0,   123,
       0,   124,     0,   124,     0,   359,     0,   360,     0,   346,
       0,   329,     0,   329,     0,   123,     0,   123,     0
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
  "MCK_DEFINE", "MCK_CAPABILITY", "MCK_ABSTRACT", "MCPT_SEMICOLON",
  "MCPT_COMMA", "MCK_ROLE", "MCK_CONDUIT", "MCK_DOMAIN", "MCK_RAIL",
  "MCK_PARTITION", "MCOP_EQUAL", "MCK_PINS", "MCOP_PLUSEQUAL",
  "MCOP_EQUALEQUAL", "MCOP_NOTEQUAL", "MCOP_LESSTHAN", "MCOP_GREATERTHAN",
  "MCOP_LESSEQTHAN", "MCOP_GREATEREQTHAN", "MCOP_DOUBLEARROW",
  "MCOP_LEFTARROW", "MCOP_RIGHTARROW", "MCOP_PLUS", "MCOP_MINUS",
  "MCOP_AND", "MCOP_OR", "MCOP_ANDAND", "MCOP_OROR", "MCOP_MULTI",
  "MCOP_DIVID", "MCOP_CARET", "MCOP_APOST", "MCPT_LBRACKET",
  "MCPT_RBRACKET", "MCPT_LPAREN", "MCPT_RPAREN", "MCOP_TILDE",
  "MCOP_PLUSMINUS", "MCOP_TIMES", "MCPT_DBCOLON", "MCK_ELSE_IF",
  "MCK_ELSE", "MCK_IF", "MC_ENDL", "MCK_RETURN", "MCK_ERROR", "MCK_IO",
  "MCK_IN", "MCK_OUT", "MCK_NC", "MCK_LABEL", "MCK_PSRC", "MCK_PSNK",
  "MCK_PSBI", "MCONST_HIGH", "MCONST_LOW", "MCONST_NC", "MCU_INT",
  "MCU_HEX", "MCU_FLOAT", "MCU_STRING", "MCK_FUNC", "MCK_THIS", "MCU_VOLT",
  "MCU_AMP", "MCU_CAP", "MCU_IND", "MCU_TIME", "MCU_LEN", "MCU_WATT",
  "MCU_OHM", "MCU_TEMP", "MCU_HZ", "MCU_DB", "MCU_PPM", "MCU_PERCENT",
  "MCU_BAUD", "MCU_DATASIZE", "MCU_SPS", "MCU_SIEMENS", "MCU_RESPONSIVITY",
  "MCU_ANGLE", "MCU_ANGULAR_RATE", "MCU_ENERGY", "MCU_EFIELD",
  "MCU_HFIELD", "MCU_FLUX", "MCU_BFIELD", "MCU_SLEW", "MCU_NOISE",
  "MCU_CHARGE", "MCUVAL_VOLT", "MCUVAL_AMP", "MCUVAL_CAP", "MCUVAL_IND",
  "MCUVAL_TIME", "MCUVAL_LEN", "MCUVAL_WATT", "MCUVAL_OHM", "MCUVAL_TEMP",
  "MCUVAL_HZ", "MCUVAL_DB", "MCUVAL_PPM", "MCUVAL_PERCENT", "MCUVAL_BAUD",
  "MCUVAL_DATASIZE", "MCUVAL_SPS", "MCUVAL_SIEMENS", "MCUVAL_RESPONSIVITY",
  "MCUVAL_ANGLE", "MCUVAL_ANGULAR_RATE", "MCUVAL_ENERGY", "MCUVAL_EFIELD",
  "MCUVAL_HFIELD", "MCUVAL_FLUX", "MCUVAL_BFIELD", "MCUVAL_SLEW",
  "MCUVAL_NOISE", "MCUVAL_CHARGE", "MC_WS", "MC_SINGLE_COMMENT",
  "MC_MULTI_COMMENT", "$accept", "start", "mc_tops", "mc_top", "mc_use",
  "mc_uri", "mc_prefix", "mc_uri_trunk", "mc_levels", "mc_class_name",
  "mc_component", "mc_module", "mc_interface", "mc_enum", "mc_define",
  "mc_capability", "mc_component_derivs", "mc_component_variant",
  "mc_component_adopts", "mc_body", "mc_clauses", "mc_clause",
  "mc_attribute", "mc_attr_values", "mc_attr_value", "mc_attr_lines",
  "mc_attribute_pin", "mc_pins_lines", "mc_pins_line", "mc_pin_idn",
  "mc_pins_names", "mc_pins_name", "mc_net", "mc_opds", "mc_net_port_elem",
  "mc_net_port_elems", "mc_mn_opd", "mc_mn_opds", "mc_opd", "mc_phrases",
  "mc_phrase", "mc_role", "mc_conduit", "mc_domain", "mc_rail",
  "mc_partition", "mc_error_stmt", "mc_error_msg", "mc_tattrs",
  "mc_tattrs_opt", "mc_tattr", "mc_tkey", "mc_resword", "mc_function",
  "mc_paramds", "mc_pards", "mc_pard", "mc_declare_a", "mc_declare_a1",
  "mc_insts", "mc_inst", "mc_iface_name", "mc_declare_b", "mc_params",
  "mc_param", "mc_conds", "mc_conds_elifs", "mc_cond_block", "mc_expr",
  "mc_judge", "mc_id", "mc_ida", "mc_idss", "mc_ids", "mc_idseg", "mc_idm",
  "mc_idans", "mc_idan", "mc_int", "mc_hex", "mc_float", "mc_number",
  "mc_string", "mc_const", "mc_nc", "mc_underscore", "mc_literal",
  "mc_iotype", "mc_unit_value", "mc_unit_type", "mc_endls", YY_NULLPTR
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
#line 186 "src/mca.y"
               {}
#line 3103 "src/mca.tab.c"
    break;
  case 3:
#line 187 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
#line 3109 "src/mca.tab.c"
    break;
  case 4:
#line 188 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3115 "src/mca.tab.c"
    break;
  case 5:
#line 189 "src/mca.y"
                                 { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
#line 3121 "src/mca.tab.c"
    break;
  case 6:
#line 190 "src/mca.y"
                        { ((*yyvalp).value) = NULL; }
#line 3127 "src/mca.tab.c"
    break;
  case 7:
#line 192 "src/mca.y"
                                 { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value); }
#line 3133 "src/mca.tab.c"
    break;
  case 8:
#line 193 "src/mca.y"
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3139 "src/mca.tab.c"
    break;
  case 9:
#line 195 "src/mca.y"
               { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3145 "src/mca.tab.c"
    break;
  case 10:
#line 196 "src/mca.y"
                     { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3151 "src/mca.tab.c"
    break;
  case 11:
#line 197 "src/mca.y"
                  { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3157 "src/mca.tab.c"
    break;
  case 12:
#line 198 "src/mca.y"
                     { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3163 "src/mca.tab.c"
    break;
  case 13:
#line 199 "src/mca.y"
                { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3169 "src/mca.tab.c"
    break;
  case 14:
#line 200 "src/mca.y"
                  { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3175 "src/mca.tab.c"
    break;
  case 15:
#line 201 "src/mca.y"
                      { mc_value_link(mcast, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3181 "src/mca.tab.c"
    break;
  case 16:
#line 202 "src/mca.y"
              { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1002_TOP_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
#line 3187 "src/mca.tab.c"
    break;
  case 17:
#line 211 "src/mca.y"
{
    
    ((*yyvalp).value) = ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->next) ? mc_value_create_node(MCAST_USE_PUB, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
#line 3196 "src/mca.tab.c"
    break;
  case 18:
#line 216 "src/mca.y"
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
#line 3214 "src/mca.tab.c"
    break;
  case 19:
#line 230 "src/mca.y"
{
    
    ((*yyvalp).value) = ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->next) ? mc_value_create_node(MCAST_USE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
#line 3223 "src/mca.tab.c"
    break;
  case 20:
#line 235 "src/mca.y"
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
#line 3241 "src/mca.tab.c"
    break;
  case 21:
#line 254 "src/mca.y"
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
#line 3253 "src/mca.tab.c"
    break;
  case 22:
#line 262 "src/mca.y"
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
#line 3265 "src/mca.tab.c"
    break;
  case 23:
#line 272 "src/mca.y"
{
    /* $1 == NULL when mc_uri_trunk recovered through `| mc_levels error`
       (bad char mid-path; E2092 already logged) — drop the whole uri. */
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) ? mc_value_link(mc_value_create_data(MCAST_URI_PREFIX, strdup("$"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
#line 3275 "src/mca.tab.c"
    break;
  case 24:
#line 278 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) ? mc_value_link3(
        mc_value_create_data(MCAST_URI_PREFIX, strdup("$"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
        mc_value_create_node(MCAST_URI_ASID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
    ) : NULL;
}
#line 3287 "src/mca.tab.c"
    break;
  case 25:
#line 286 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value) ? mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)) : NULL;
}
#line 3295 "src/mca.tab.c"
    break;
  case 26:
#line 290 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value) ? mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_URI_ASID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            ) : NULL;
}
#line 3307 "src/mca.tab.c"
    break;
  case 27:
#line 299 "src/mca.y"
                                { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("/"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 3313 "src/mca.tab.c"
    break;
  case 28:
#line 300 "src/mca.y"
                                { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("./"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen);}
#line 3319 "src/mca.tab.c"
    break;
  case 29:
#line 301 "src/mca.y"
                                         { ((*yyvalp).value) = mc_value_create_data(MCAST_URI_PREFIX, strdup("../"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 3325 "src/mca.tab.c"
    break;
  case 30:
#line 304 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3333 "src/mca.tab.c"
    break;
  case 31:
#line 308 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 3342 "src/mca.tab.c"
    break;
  case 32:
#line 313 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_MODULE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 3351 "src/mca.tab.c"
    break;
  case 33:
#line 318 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
#line 3360 "src/mca.tab.c"
    break;
  case 34:
#line 323 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)), 
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
#line 3370 "src/mca.tab.c"
    break;
  case 35:
#line 329 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_link( mc_value_create_node(MCAST_URI_FILE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                       mc_value_create_data(MCAST_URI_VERSION, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen));
    ((*yyvalp).value)->len += (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen;  // extend position to include ".mc"
}
#line 3380 "src/mca.tab.c"
    break;
  case 36:
#line 340 "src/mca.y"
{
    
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1013_USE_ERROR, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
#line 3393 "src/mca.tab.c"
    break;
  case 37:
#line 355 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3399 "src/mca.tab.c"
    break;
  case 38:
#line 356 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3405 "src/mca.tab.c"
    break;
  case 39:
#line 357 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3411 "src/mca.tab.c"
    break;
  case 40:
#line 358 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3417 "src/mca.tab.c"
    break;
  case 41:
#line 359 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3423 "src/mca.tab.c"
    break;
  case 42:
#line 360 "src/mca.y"
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3429 "src/mca.tab.c"
    break;
  case 43:
#line 364 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 3435 "src/mca.tab.c"
    break;
  case 44:
#line 365 "src/mca.y"
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);}
#line 3441 "src/mca.tab.c"
    break;
  case 45:
#line 375 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COMPONENT, mc_value_link(
            mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 3456 "src/mca.tab.c"
    break;
  case 46:
#line 386 "src/mca.y"
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
#line 3473 "src/mca.tab.c"
    break;
  case 47:
#line 400 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_MODULE, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 3486 "src/mca.tab.c"
    break;
  case 48:
#line 410 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INTERFACE, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 3499 "src/mca.tab.c"
    break;
  case 49:
#line 420 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ENUM, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)), 
                mc_value_create_node(MCAST_ENUM_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))
            ));
}
#line 3511 "src/mca.tab.c"
    break;
  case 50:
#line 429 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DEFINE, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 3520 "src/mca.tab.c"
    break;
  case 51:
#line 435 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_CAPABILITY, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 3529 "src/mca.tab.c"
    break;
  case 52:
#line 443 "src/mca.y"
                   { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3535 "src/mca.tab.c"
    break;
  case 53:
#line 446 "src/mca.y"
                    { ((*yyvalp).value) = mc_value_create_node(MCAST_VARIANT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3541 "src/mca.tab.c"
    break;
  case 54:
#line 447 "src/mca.y"
                                               { ((*yyvalp).value) = NULL; }
#line 3547 "src/mca.tab.c"
    break;
  case 55:
#line 450 "src/mca.y"
                    { ((*yyvalp).value) = mc_value_create_node(MCAST_ADOPTS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3553 "src/mca.tab.c"
    break;
  case 56:
#line 451 "src/mca.y"
                                               { ((*yyvalp).value) = NULL; }
#line 3559 "src/mca.tab.c"
    break;
  case 57:
#line 455 "src/mca.y"
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1105_EMPTY_BODY, 2,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_BODY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 3573 "src/mca.tab.c"
    break;
  case 58:
#line 467 "src/mca.y"
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3579 "src/mca.tab.c"
    break;
  case 59:
#line 468 "src/mca.y"
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3585 "src/mca.tab.c"
    break;
  case 60:
#line 469 "src/mca.y"
                                            { ((*yyvalp).value) = NULL; }
#line 3591 "src/mca.tab.c"
    break;
  case 61:
#line 471 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3597 "src/mca.tab.c"
    break;
  case 62:
#line 472 "src/mca.y"
                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3603 "src/mca.tab.c"
    break;
  case 63:
#line 473 "src/mca.y"
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3609 "src/mca.tab.c"
    break;
  case 64:
#line 474 "src/mca.y"
                   { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3615 "src/mca.tab.c"
    break;
  case 65:
#line 475 "src/mca.y"
                       { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3621 "src/mca.tab.c"
    break;
  case 66:
#line 476 "src/mca.y"
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3627 "src/mca.tab.c"
    break;
  case 67:
#line 477 "src/mca.y"
                      { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3633 "src/mca.tab.c"
    break;
  case 68:
#line 478 "src/mca.y"
                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3639 "src/mca.tab.c"
    break;
  case 69:
#line 479 "src/mca.y"
                   { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3645 "src/mca.tab.c"
    break;
  case 70:
#line 480 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3651 "src/mca.tab.c"
    break;
  case 71:
#line 481 "src/mca.y"
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3657 "src/mca.tab.c"
    break;
  case 72:
#line 482 "src/mca.y"
                 { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1003_CLAUSE_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
#line 3663 "src/mca.tab.c"
    break;
  case 73:
#line 486 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3674 "src/mca.tab.c"
    break;
  case 74:
#line 505 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link(
                            mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                            mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3688 "src/mca.tab.c"
    break;
  case 75:
#line 516 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3697 "src/mca.tab.c"
    break;
  case 76:
#line 521 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 3706 "src/mca.tab.c"
    break;
  case 77:
#line 528 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
#line 3714 "src/mca.tab.c"
    break;
  case 78:
#line 532 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
#line 3722 "src/mca.tab.c"
    break;
  case 79:
#line 536 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); 
}
#line 3730 "src/mca.tab.c"
    break;
  case 80:
#line 540 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 3739 "src/mca.tab.c"
    break;
  case 81:
#line 545 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3745 "src/mca.tab.c"
    break;
  case 82:
#line 546 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3751 "src/mca.tab.c"
    break;
  case 83:
#line 551 "src/mca.y"
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 3765 "src/mca.tab.c"
    break;
  case 84:
#line 561 "src/mca.y"
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PINADD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 3779 "src/mca.tab.c"
    break;
  case 85:
#line 571 "src/mca.y"
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 3793 "src/mca.tab.c"
    break;
  case 86:
#line 583 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 3799 "src/mca.tab.c"
    break;
  case 87:
#line 584 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 3805 "src/mca.tab.c"
    break;
  case 88:
#line 585 "src/mca.y"
                                                    { ((*yyvalp).value) = NULL; }
#line 3811 "src/mca.tab.c"
    break;
  case 89:
#line 588 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3822 "src/mca.tab.c"
    break;
  case 90:
#line 595 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3834 "src/mca.tab.c"
    break;
  case 91:
#line 603 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3846 "src/mca.tab.c"
    break;
  case 92:
#line 611 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
#line 3860 "src/mca.tab.c"
    break;
  case 93:
#line 621 "src/mca.y"
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
#line 3876 "src/mca.tab.c"
    break;
  case 94:
#line 633 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 3889 "src/mca.tab.c"
    break;
  case 95:
#line 642 "src/mca.y"
{
    
    // trailing `@attr…` run plus a `, label…` value tail (golden hs analog
    // supply row: `psnk [13,12] = [VDDA,VSSA]::DC(5V) @class(analog), "..."`).
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3904 "src/mca.tab.c"
    break;
  case 96:
#line 653 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_link4(
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value),
                    mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 3919 "src/mca.tab.c"
    break;
  case 97:
#line 664 "src/mca.y"
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1004_PIN_SKIPPED, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
#line 3931 "src/mca.tab.c"
    break;
  case 98:
#line 674 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 3940 "src/mca.tab.c"
    break;
  case 99:
#line 679 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 3949 "src/mca.tab.c"
    break;
  case 100:
#line 684 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3958 "src/mca.tab.c"
    break;
  case 101:
#line 689 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3967 "src/mca.tab.c"
    break;
  case 102:
#line 694 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 3976 "src/mca.tab.c"
    break;
  case 103:
#line 701 "src/mca.y"
{ 
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3984 "src/mca.tab.c"
    break;
  case 104:
#line 705 "src/mca.y"
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 3993 "src/mca.tab.c"
    break;
  case 105:
#line 710 "src/mca.y"
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4002 "src/mca.tab.c"
    break;
  case 106:
#line 715 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4011 "src/mca.tab.c"
    break;
  case 107:
#line 723 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4020 "src/mca.tab.c"
    break;
  case 108:
#line 728 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4029 "src/mca.tab.c"
    break;
  case 109:
#line 733 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4038 "src/mca.tab.c"
    break;
  case 110:
#line 738 "src/mca.y"
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 4 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 6 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 7 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 9)) {
        mc_dlog_add(MCD_E1007_NET_NOT_PORT, 1,
                    g_last_token->tpos, g_last_token->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4052 "src/mca.tab.c"
    break;
  case 111:
#line 747 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link(
            mc_value_create_data(MCAST_IOTYPE_RETURN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 4064 "src/mca.tab.c"
    break;
  case 112:
#line 759 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 4070 "src/mca.tab.c"
    break;
  case 113:
#line 760 "src/mca.y"
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4076 "src/mca.tab.c"
    break;
  case 114:
#line 774 "src/mca.y"
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4082 "src/mca.tab.c"
    break;
  case 115:
#line 775 "src/mca.y"
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4088 "src/mca.tab.c"
    break;
  case 116:
#line 776 "src/mca.y"
                                                                 { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 4094 "src/mca.tab.c"
    break;
  case 117:
#line 777 "src/mca.y"
                                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4100 "src/mca.tab.c"
    break;
  case 120:
#line 796 "src/mca.y"
          {
              
              ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
          }
#line 4109 "src/mca.tab.c"
    break;
  case 121:
#line 800 "src/mca.y"
                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 4115 "src/mca.tab.c"
    break;
  case 122:
#line 801 "src/mca.y"
                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4121 "src/mca.tab.c"
    break;
  case 123:
#line 805 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4130 "src/mca.tab.c"
    break;
  case 124:
#line 810 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 4139 "src/mca.tab.c"
    break;
  case 125:
#line 815 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, 
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 4149 "src/mca.tab.c"
    break;
  case 126:
#line 821 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4159 "src/mca.tab.c"
    break;
  case 127:
#line 827 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 4171 "src/mca.tab.c"
    break;
  case 128:
#line 835 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 4184 "src/mca.tab.c"
    break;
  case 129:
#line 847 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD,
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 4194 "src/mca.tab.c"
    break;
  case 130:
#line 853 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4204 "src/mca.tab.c"
    break;
  case 131:
#line 859 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 4216 "src/mca.tab.c"
    break;
  case 132:
#line 867 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 4229 "src/mca.tab.c"
    break;
  case 133:
#line 876 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4238 "src/mca.tab.c"
    break;
  case 134:
#line 881 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 4247 "src/mca.tab.c"
    break;
  case 135:
#line 888 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4256 "src/mca.tab.c"
    break;
  case 136:
#line 893 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4265 "src/mca.tab.c"
    break;
  case 137:
#line 898 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 4274 "src/mca.tab.c"
    break;
  case 138:
#line 902 "src/mca.y"
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4280 "src/mca.tab.c"
    break;
  case 139:
#line 903 "src/mca.y"
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4286 "src/mca.tab.c"
    break;
  case 140:
#line 904 "src/mca.y"
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4292 "src/mca.tab.c"
    break;
  case 141:
#line 909 "src/mca.y"
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4298 "src/mca.tab.c"
    break;
  case 142:
#line 910 "src/mca.y"
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 4304 "src/mca.tab.c"
    break;
  case 143:
#line 912 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
#line 4310 "src/mca.tab.c"
    break;
  case 144:
#line 913 "src/mca.y"
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1103_TRANSPOSE_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
#line 4316 "src/mca.tab.c"
    break;
  case 145:
#line 914 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
#line 4322 "src/mca.tab.c"
    break;
  case 146:
#line 915 "src/mca.y"
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1104_CARET_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
#line 4328 "src/mca.tab.c"
    break;
  case 147:
#line 917 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4334 "src/mca.tab.c"
    break;
  case 148:
#line 918 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4340 "src/mca.tab.c"
    break;
  case 149:
#line 919 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4346 "src/mca.tab.c"
    break;
  case 150:
#line 920 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4352 "src/mca.tab.c"
    break;
  case 151:
#line 921 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4358 "src/mca.tab.c"
    break;
  case 152:
#line 922 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4364 "src/mca.tab.c"
    break;
  case 153:
#line 923 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4370 "src/mca.tab.c"
    break;
  case 154:
#line 924 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4376 "src/mca.tab.c"
    break;
  case 155:
#line 925 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4382 "src/mca.tab.c"
    break;
  case 156:
#line 927 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4388 "src/mca.tab.c"
    break;
  case 157:
#line 928 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4394 "src/mca.tab.c"
    break;
  case 158:
#line 929 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4400 "src/mca.tab.c"
    break;
  case 159:
#line 930 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4406 "src/mca.tab.c"
    break;
  case 160:
#line 931 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4412 "src/mca.tab.c"
    break;
  case 161:
#line 932 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4418 "src/mca.tab.c"
    break;
  case 162:
#line 933 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4424 "src/mca.tab.c"
    break;
  case 163:
#line 934 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4430 "src/mca.tab.c"
    break;
  case 164:
#line 935 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4436 "src/mca.tab.c"
    break;
  case 165:
#line 937 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4442 "src/mca.tab.c"
    break;
  case 166:
#line 938 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4448 "src/mca.tab.c"
    break;
  case 167:
#line 939 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4454 "src/mca.tab.c"
    break;
  case 168:
#line 940 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4460 "src/mca.tab.c"
    break;
  case 169:
#line 941 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4466 "src/mca.tab.c"
    break;
  case 170:
#line 942 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4472 "src/mca.tab.c"
    break;
  case 171:
#line 943 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4478 "src/mca.tab.c"
    break;
  case 172:
#line 944 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4484 "src/mca.tab.c"
    break;
  case 173:
#line 945 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4490 "src/mca.tab.c"
    break;
  case 174:
#line 954 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4496 "src/mca.tab.c"
    break;
  case 175:
#line 955 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4502 "src/mca.tab.c"
    break;
  case 176:
#line 956 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4508 "src/mca.tab.c"
    break;
  case 177:
#line 958 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4514 "src/mca.tab.c"
    break;
  case 178:
#line 959 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4520 "src/mca.tab.c"
    break;
  case 179:
#line 960 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4526 "src/mca.tab.c"
    break;
  case 180:
#line 961 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4532 "src/mca.tab.c"
    break;
  case 181:
#line 962 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4538 "src/mca.tab.c"
    break;
  case 182:
#line 963 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4544 "src/mca.tab.c"
    break;
  case 183:
#line 964 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4550 "src/mca.tab.c"
    break;
  case 184:
#line 965 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4556 "src/mca.tab.c"
    break;
  case 185:
#line 966 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4562 "src/mca.tab.c"
    break;
  case 186:
#line 968 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4568 "src/mca.tab.c"
    break;
  case 187:
#line 969 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4574 "src/mca.tab.c"
    break;
  case 188:
#line 970 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4580 "src/mca.tab.c"
    break;
  case 189:
#line 971 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4586 "src/mca.tab.c"
    break;
  case 190:
#line 972 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4592 "src/mca.tab.c"
    break;
  case 191:
#line 973 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4598 "src/mca.tab.c"
    break;
  case 192:
#line 974 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4604 "src/mca.tab.c"
    break;
  case 193:
#line 975 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4610 "src/mca.tab.c"
    break;
  case 194:
#line 976 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4616 "src/mca.tab.c"
    break;
  case 195:
#line 978 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4622 "src/mca.tab.c"
    break;
  case 196:
#line 979 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4628 "src/mca.tab.c"
    break;
  case 197:
#line 980 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4634 "src/mca.tab.c"
    break;
  case 198:
#line 981 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4640 "src/mca.tab.c"
    break;
  case 199:
#line 982 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4646 "src/mca.tab.c"
    break;
  case 200:
#line 983 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4652 "src/mca.tab.c"
    break;
  case 201:
#line 984 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4658 "src/mca.tab.c"
    break;
  case 202:
#line 985 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4664 "src/mca.tab.c"
    break;
  case 203:
#line 986 "src/mca.y"
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4670 "src/mca.tab.c"
    break;
  case 204:
#line 988 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4676 "src/mca.tab.c"
    break;
  case 205:
#line 989 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4682 "src/mca.tab.c"
    break;
  case 206:
#line 990 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4688 "src/mca.tab.c"
    break;
  case 207:
#line 991 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4694 "src/mca.tab.c"
    break;
  case 208:
#line 993 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4700 "src/mca.tab.c"
    break;
  case 209:
#line 994 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4706 "src/mca.tab.c"
    break;
  case 210:
#line 995 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4712 "src/mca.tab.c"
    break;
  case 211:
#line 996 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 4718 "src/mca.tab.c"
    break;
  case 212:
#line 999 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4727 "src/mca.tab.c"
    break;
  case 213:
#line 1004 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 4736 "src/mca.tab.c"
    break;
  case 214:
#line 1009 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 4745 "src/mca.tab.c"
    break;
  case 215:
#line 1014 "src/mca.y"
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
#line 4768 "src/mca.tab.c"
    break;
  case 216:
#line 1033 "src/mca.y"
{
    
    // Wrap each mc_opds in an MCAST_OPDS node: mc_value_link3 flattens the
    // whole next-chain, so without the wrapper the `|` boundary between the
    // left/right option lists is lost (left side absorbs the right side's ids).
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
#line 4783 "src/mca.tab.c"
    break;
  case 217:
#line 1044 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
#line 4795 "src/mca.tab.c"
    break;
  case 218:
#line 1053 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 4804 "src/mca.tab.c"
    break;
  case 219:
#line 1058 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
#line 4813 "src/mca.tab.c"
    break;
  case 220:
#line 1065 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
#line 4825 "src/mca.tab.c"
    break;
  case 221:
#line 1073 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
#line 4838 "src/mca.tab.c"
    break;
  case 222:
#line 1083 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
#line 4851 "src/mca.tab.c"
    break;
  case 223:
#line 1093 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
#line 4864 "src/mca.tab.c"
    break;
  case 224:
#line 1102 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
#line 4877 "src/mca.tab.c"
    break;
  case 225:
#line 1111 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
#line 4889 "src/mca.tab.c"
    break;
  case 226:
#line 1119 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
#line 4901 "src/mca.tab.c"
    break;
  case 227:
#line 1127 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
#line 4913 "src/mca.tab.c"
    break;
  case 228:
#line 1135 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
#line 4925 "src/mca.tab.c"
    break;
  case 229:
#line 1145 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 4938 "src/mca.tab.c"
    break;
  case 230:
#line 1154 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 4952 "src/mca.tab.c"
    break;
  case 231:
#line 1165 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 4966 "src/mca.tab.c"
    break;
  case 232:
#line 1176 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 4980 "src/mca.tab.c"
    break;
  case 233:
#line 1186 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 4993 "src/mca.tab.c"
    break;
  case 234:
#line 1195 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 5006 "src/mca.tab.c"
    break;
  case 235:
#line 1204 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 5019 "src/mca.tab.c"
    break;
  case 236:
#line 1213 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
#line 5032 "src/mca.tab.c"
    break;
  case 237:
#line 1247 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5043 "src/mca.tab.c"
    break;
  case 238:
#line 1254 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5056 "src/mca.tab.c"
    break;
  case 239:
#line 1263 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5067 "src/mca.tab.c"
    break;
  case 240:
#line 1270 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5080 "src/mca.tab.c"
    break;
  case 241:
#line 1281 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5089 "src/mca.tab.c"
    break;
  case 242:
#line 1286 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5098 "src/mca.tab.c"
    break;
  case 243:
#line 1294 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ROLE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5107 "src/mca.tab.c"
    break;
  case 244:
#line 1308 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_REF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));   // MCAST_REF kept as internal node name
}
#line 5116 "src/mca.tab.c"
    break;
  case 245:
#line 1314 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DOMAIN, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5125 "src/mca.tab.c"
    break;
  case 246:
#line 1320 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_RAIL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5134 "src/mca.tab.c"
    break;
  case 247:
#line 1337 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARTITION, mc_value_link3(
                    mc_value_create_data(MCAST_PARTITION_LEVEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5146 "src/mca.tab.c"
    break;
  case 248:
#line 1354 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ERROR, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5155 "src/mca.tab.c"
    break;
  case 249:
#line 1364 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5163 "src/mca.tab.c"
    break;
  case 250:
#line 1368 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5171 "src/mca.tab.c"
    break;
  case 251:
#line 1372 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5179 "src/mca.tab.c"
    break;
  case 252:
#line 1376 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5187 "src/mca.tab.c"
    break;
  case 253:
#line 1380 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5195 "src/mca.tab.c"
    break;
  case 254:
#line 1384 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5203 "src/mca.tab.c"
    break;
  case 255:
#line 1397 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5212 "src/mca.tab.c"
    break;
  case 256:
#line 1402 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5221 "src/mca.tab.c"
    break;
  case 257:
#line 1408 "src/mca.y"
{
    ((*yyvalp).value) = NULL;
}
#line 5229 "src/mca.tab.c"
    break;
  case 258:
#line 1412 "src/mca.y"
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5237 "src/mca.tab.c"
    break;
  case 259:
#line 1417 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5246 "src/mca.tab.c"
    break;
  case 260:
#line 1422 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
#line 5257 "src/mca.tab.c"
    break;
  case 261:
#line 1433 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5265 "src/mca.tab.c"
    break;
  case 262:
#line 1437 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 5273 "src/mca.tab.c"
    break;
  case 263:
#line 1441 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 5281 "src/mca.tab.c"
    break;
  case 271:
#line 1455 "src/mca.y"
{
    char buf[160];
    snprintf(buf, sizeof buf,
             "Reserved word '%s' cannot be an attribute value (reserved words "
             "are not names anywhere); pick another spelling.", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring);
    mc_dlog_add(MCD_E1032_TATTR_RESWORD, 1, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, buf);
    ((*yyvalp).value) = NULL;
}
#line 5294 "src/mca.tab.c"
    break;
  case 272:
#line 1467 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_FUNCTION, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5306 "src/mca.tab.c"
    break;
  case 273:
#line 1478 "src/mca.y"
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
#line 5312 "src/mca.tab.c"
    break;
  case 274:
#line 1479 "src/mca.y"
                                                { ((*yyvalp).value) = NULL; }
#line 5318 "src/mca.tab.c"
    break;
  case 275:
#line 1480 "src/mca.y"
                                                { ((*yyvalp).value) = NULL; }
#line 5324 "src/mca.tab.c"
    break;
  case 276:
#line 1482 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5330 "src/mca.tab.c"
    break;
  case 277:
#line 1483 "src/mca.y"
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5336 "src/mca.tab.c"
    break;
  case 278:
#line 1488 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 5344 "src/mca.tab.c"
    break;
  case 279:
#line 1492 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link(
        mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5355 "src/mca.tab.c"
    break;
  case 280:
#line 1500 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5364 "src/mca.tab.c"
    break;
  case 281:
#line 1506 "src/mca.y"
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
#line 5373 "src/mca.tab.c"
    break;
  case 282:
#line 1513 "src/mca.y"
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
#line 5382 "src/mca.tab.c"
    break;
  case 283:
#line 1519 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5391 "src/mca.tab.c"
    break;
  case 284:
#line 1525 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5400 "src/mca.tab.c"
    break;
  case 285:
#line 1531 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
#line 5409 "src/mca.tab.c"
    break;
  case 286:
#line 1537 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5418 "src/mca.tab.c"
    break;
  case 287:
#line 1543 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5427 "src/mca.tab.c"
    break;
  case 288:
#line 1549 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5436 "src/mca.tab.c"
    break;
  case 289:
#line 1555 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5445 "src/mca.tab.c"
    break;
  case 290:
#line 1561 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value))))
        );
}
#line 5458 "src/mca.tab.c"
    break;
  case 291:
#line 1571 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))
        );
}
#line 5471 "src/mca.tab.c"
    break;
  case 292:
#line 1580 "src/mca.y"
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
#line 5487 "src/mca.tab.c"
    break;
  case 293:
#line 1596 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5499 "src/mca.tab.c"
    break;
  case 294:
#line 1608 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5511 "src/mca.tab.c"
    break;
  case 295:
#line 1616 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5523 "src/mca.tab.c"
    break;
  case 296: /* mc_declare_a1: mc_ids mc_inst  */
#line 1625 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5535 "src/mca.tab.c"
    break;
  case 297: /* mc_declare_a1: mc_ids MCPT_DOT mc_int mc_inst  */
#line 1633 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5547 "src/mca.tab.c"
    break;
  case 298: /* mc_declare_a1: mc_ids MCPT_LPAREN mc_params MCPT_RPAREN mc_inst  */
#line 1641 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
#line 5559 "src/mca.tab.c"
    break;
  case 299:
#line 1650 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5568 "src/mca.tab.c"
    break;
  case 300:
#line 1655 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5577 "src/mca.tab.c"
    break;
  case 301:
#line 1662 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5586 "src/mca.tab.c"
    break;
  case 302:
#line 1667 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
#line 5595 "src/mca.tab.c"
    break;
  case 304:
#line 1677 "src/mca.y"
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5601 "src/mca.tab.c"
    break;
  case 305:
#line 1679 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value))));
}
#line 5612 "src/mca.tab.c"
    break;
  case 306:
#line 1686 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                        mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                        mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-8)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value))))
                    ));
}
#line 5624 "src/mca.tab.c"
    break;
  case 307:
#line 1694 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)))));
}
#line 5635 "src/mca.tab.c"
    break;
  case 308:
#line 1703 "src/mca.y"
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5641 "src/mca.tab.c"
    break;
  case 309:
#line 1704 "src/mca.y"
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5647 "src/mca.tab.c"
    break;
  case 310:
#line 1705 "src/mca.y"
                                            { ((*yyvalp).value) = NULL; }
#line 5653 "src/mca.tab.c"
    break;
  case 311:
#line 1709 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5661 "src/mca.tab.c"
    break;
  case 312:
#line 1713 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5669 "src/mca.tab.c"
    break;
  case 313:
#line 1717 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
#line 5677 "src/mca.tab.c"
    break;
  case 314:
#line 1721 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5685 "src/mca.tab.c"
    break;
  case 315:
#line 1729 "src/mca.y"
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
#line 5693 "src/mca.tab.c"
    break;
  case 316:
#line 1736 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
#line 5704 "src/mca.tab.c"
    break;
  case 317:
#line 1748 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
#line 5715 "src/mca.tab.c"
    break;
  case 318:
#line 1759 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
            mc_value_link(
                mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
#line 5729 "src/mca.tab.c"
    break;
  case 319:
#line 1771 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_IF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5738 "src/mca.tab.c"
    break;
  case 320:
#line 1776 "src/mca.y"
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), 
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 5751 "src/mca.tab.c"
    break;
  case 321:
#line 1785 "src/mca.y"
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5760 "src/mca.tab.c"
    break;
  case 322:
#line 1790 "src/mca.y"
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link4(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
#line 5774 "src/mca.tab.c"
    break;
  case 323:
#line 1802 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
#line 5783 "src/mca.tab.c"
    break;
  case 324:
#line 1807 "src/mca.y"
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
#line 5792 "src/mca.tab.c"
    break;
  case 325:
#line 1814 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5801 "src/mca.tab.c"
    break;
  case 326:
#line 1819 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5810 "src/mca.tab.c"
    break;
  case 327:
#line 1824 "src/mca.y"
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
#line 5819 "src/mca.tab.c"
    break;
  case 328:
#line 1830 "src/mca.y"
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5825 "src/mca.tab.c"
    break;
  case 329:
#line 1831 "src/mca.y"
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5831 "src/mca.tab.c"
    break;
  case 330:
#line 1832 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5837 "src/mca.tab.c"
    break;
  case 331:
#line 1834 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_EQEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5843 "src/mca.tab.c"
    break;
  case 332:
#line 1835 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_AND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5849 "src/mca.tab.c"
    break;
  case 333:
#line 1836 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_OR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5855 "src/mca.tab.c"
    break;
  case 334:
#line 1837 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_NOTEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5861 "src/mca.tab.c"
    break;
  case 335:
#line 1838 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));}
#line 5867 "src/mca.tab.c"
    break;
  case 336:
#line 1839 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATERTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5873 "src/mca.tab.c"
    break;
  case 337:
#line 1840 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSEQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5879 "src/mca.tab.c"
    break;
  case 338:
#line 1841 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATEREQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5885 "src/mca.tab.c"
    break;
  case 339:
#line 1842 "src/mca.y"
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITAND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5891 "src/mca.tab.c"
    break;
  case 340:
#line 1843 "src/mca.y"
                                                { mc_dlog_add(MCD_W1101_SINGLE_OR, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITOR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5897 "src/mca.tab.c"
    break;
  case 341:
#line 1844 "src/mca.y"
                                                { mc_dlog_add(MCD_W1102_PLUSMINUS, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 5903 "src/mca.tab.c"
    break;
  case 342:
#line 1845 "src/mca.y"
                                                               { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_IN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))); }
#line 5909 "src/mca.tab.c"
    break;
  case 343:
#line 1846 "src/mca.y"
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
#line 5915 "src/mca.tab.c"
    break;
  case 344:
#line 1851 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 5921 "src/mca.tab.c"
    break;
  case 345:
#line 1852 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 5927 "src/mca.tab.c"
    break;
  case 346:
#line 1853 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5933 "src/mca.tab.c"
    break;
  case 347:
#line 1855 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5939 "src/mca.tab.c"
    break;
  case 348:
#line 1856 "src/mca.y"
                                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
#line 5945 "src/mca.tab.c"
    break;
  case 349:
#line 1857 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5951 "src/mca.tab.c"
    break;
  case 350:
#line 1860 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5957 "src/mca.tab.c"
    break;
  case 351:
#line 1861 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5963 "src/mca.tab.c"
    break;
  case 352:
#line 1862 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5969 "src/mca.tab.c"
    break;
  case 353:
#line 1863 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5975 "src/mca.tab.c"
    break;
  case 354:
#line 1864 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5981 "src/mca.tab.c"
    break;
  case 355:
#line 1865 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
#line 5987 "src/mca.tab.c"
    break;
  case 356:
#line 1866 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 5993 "src/mca.tab.c"
    break;
  case 357:
#line 1867 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 5999 "src/mca.tab.c"
    break;
  case 358:
#line 1868 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
#line 6005 "src/mca.tab.c"
    break;
  case 359:
#line 1869 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6011 "src/mca.tab.c"
    break;
  case 360:
#line 1870 "src/mca.y"
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6017 "src/mca.tab.c"
    break;
  case 361:
#line 1871 "src/mca.y"
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
#line 6023 "src/mca.tab.c"
    break;
  case 362:
#line 1873 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6029 "src/mca.tab.c"
    break;
  case 363:
#line 1874 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6035 "src/mca.tab.c"
    break;
  case 364:
#line 1875 "src/mca.y"
                            { ((*yyvalp).value) = mc_value_create_data(MCAST_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6041 "src/mca.tab.c"
    break;
  case 365:
#line 1876 "src/mca.y"
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6047 "src/mca.tab.c"
    break;
  case 366:
#line 1877 "src/mca.y"
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6053 "src/mca.tab.c"
    break;
  case 367:
#line 1878 "src/mca.y"
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6059 "src/mca.tab.c"
    break;
  case 368:
#line 1880 "src/mca.y"
                       { ((*yyvalp).value) = mc_value_create_data(MCAST_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6065 "src/mca.tab.c"
    break;
  case 369:
#line 1882 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6071 "src/mca.tab.c"
    break;
  case 370:
#line 1883 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6077 "src/mca.tab.c"
    break;
  case 371:
#line 1884 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6083 "src/mca.tab.c"
    break;
  case 372:
#line 1885 "src/mca.y"
                               { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_USCORE, strdup("_"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6089 "src/mca.tab.c"
    break;
  case 373:
#line 1890 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6095 "src/mca.tab.c"
    break;
  case 374:
#line 1891 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6101 "src/mca.tab.c"
    break;
  case 375:
#line 1892 "src/mca.y"
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
#line 6107 "src/mca.tab.c"
    break;
  case 376:
#line 1893 "src/mca.y"
                        {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
#line 6116 "src/mca.tab.c"
    break;
  case 377:
#line 1897 "src/mca.y"
                                              {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
#line 6125 "src/mca.tab.c"
    break;
  case 378:
#line 1901 "src/mca.y"
                                       {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
#line 6134 "src/mca.tab.c"
    break;
  case 379:
#line 1905 "src/mca.y"
                                   {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
#line 6143 "src/mca.tab.c"
    break;
  case 380:
#line 1911 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6149 "src/mca.tab.c"
    break;
  case 381:
#line 1912 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_OUT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6155 "src/mca.tab.c"
    break;
  case 382:
#line 1913 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IO, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6161 "src/mca.tab.c"
    break;
  case 383:
#line 1914 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSRC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6167 "src/mca.tab.c"
    break;
  case 384:
#line 1915 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSNK, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6173 "src/mca.tab.c"
    break;
  case 385:
#line 1916 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSBI, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6179 "src/mca.tab.c"
    break;
  case 386:
#line 1917 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6185 "src/mca.tab.c"
    break;
  case 387:
#line 1918 "src/mca.y"
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_LABEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
#line 6191 "src/mca.tab.c"
    break;
  case 388:
#line 1925 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6197 "src/mca.tab.c"
    break;
  case 389:
#line 1926 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6203 "src/mca.tab.c"
    break;
  case 390:
#line 1927 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6209 "src/mca.tab.c"
    break;
  case 391:
#line 1928 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6215 "src/mca.tab.c"
    break;
  case 392:
#line 1929 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6221 "src/mca.tab.c"
    break;
  case 393:
#line 1930 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6227 "src/mca.tab.c"
    break;
  case 394:
#line 1931 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6233 "src/mca.tab.c"
    break;
  case 395:
#line 1932 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6239 "src/mca.tab.c"
    break;
  case 396:
#line 1933 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6245 "src/mca.tab.c"
    break;
  case 397:
#line 1934 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6251 "src/mca.tab.c"
    break;
  case 398:
#line 1935 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6257 "src/mca.tab.c"
    break;
  case 399:
#line 1936 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6263 "src/mca.tab.c"
    break;
  case 400:
#line 1937 "src/mca.y"
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6269 "src/mca.tab.c"
    break;
  case 401:
#line 1938 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6275 "src/mca.tab.c"
    break;
  case 402:
#line 1939 "src/mca.y"
                  { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6281 "src/mca.tab.c"
    break;
  case 403:
#line 1940 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6287 "src/mca.tab.c"
    break;
  case 404:
#line 1941 "src/mca.y"
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6293 "src/mca.tab.c"
    break;
  case 405:
#line 1942 "src/mca.y"
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6299 "src/mca.tab.c"
    break;
  case 406:
#line 1943 "src/mca.y"
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6305 "src/mca.tab.c"
    break;
  case 407:
#line 1944 "src/mca.y"
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6311 "src/mca.tab.c"
    break;
  case 408:
#line 1945 "src/mca.y"
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6317 "src/mca.tab.c"
    break;
  case 409:
#line 1946 "src/mca.y"
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6323 "src/mca.tab.c"
    break;
  case 410:
#line 1947 "src/mca.y"
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6329 "src/mca.tab.c"
    break;
  case 411:
#line 1948 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6335 "src/mca.tab.c"
    break;
  case 412:
#line 1949 "src/mca.y"
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6341 "src/mca.tab.c"
    break;
  case 413:
#line 1950 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6347 "src/mca.tab.c"
    break;
  case 414:
#line 1951 "src/mca.y"
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6353 "src/mca.tab.c"
    break;
  case 415:
#line 1952 "src/mca.y"
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6359 "src/mca.tab.c"
    break;
  case 416:
#line 1956 "src/mca.y"
        { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6365 "src/mca.tab.c"
    break;
  case 417:
#line 1957 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6371 "src/mca.tab.c"
    break;
  case 418:
#line 1958 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6377 "src/mca.tab.c"
    break;
  case 419:
#line 1959 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6383 "src/mca.tab.c"
    break;
  case 420:
#line 1960 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6389 "src/mca.tab.c"
    break;
  case 421:
#line 1961 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6395 "src/mca.tab.c"
    break;
  case 422:
#line 1962 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6401 "src/mca.tab.c"
    break;
  case 423:
#line 1963 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6407 "src/mca.tab.c"
    break;
  case 424:
#line 1964 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6413 "src/mca.tab.c"
    break;
  case 425:
#line 1965 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6419 "src/mca.tab.c"
    break;
  case 426:
#line 1966 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6425 "src/mca.tab.c"
    break;
  case 427:
#line 1967 "src/mca.y"
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6431 "src/mca.tab.c"
    break;
  case 428:
#line 1968 "src/mca.y"
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6437 "src/mca.tab.c"
    break;
  case 429:
#line 1969 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6443 "src/mca.tab.c"
    break;
  case 430:
#line 1970 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6449 "src/mca.tab.c"
    break;
  case 431:
#line 1971 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6455 "src/mca.tab.c"
    break;
  case 432:
#line 1972 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6461 "src/mca.tab.c"
    break;
  case 433:
#line 1973 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6467 "src/mca.tab.c"
    break;
  case 434:
#line 1974 "src/mca.y"
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6473 "src/mca.tab.c"
    break;
  case 435:
#line 1975 "src/mca.y"
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6479 "src/mca.tab.c"
    break;
  case 436:
#line 1976 "src/mca.y"
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6485 "src/mca.tab.c"
    break;
  case 437:
#line 1977 "src/mca.y"
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6491 "src/mca.tab.c"
    break;
  case 438:
#line 1978 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6497 "src/mca.tab.c"
    break;
  case 439:
#line 1979 "src/mca.y"
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6503 "src/mca.tab.c"
    break;
  case 440:
#line 1980 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6509 "src/mca.tab.c"
    break;
  case 441:
#line 1981 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6515 "src/mca.tab.c"
    break;
  case 442:
#line 1982 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6521 "src/mca.tab.c"
    break;
  case 443:
#line 1983 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6527 "src/mca.tab.c"
    break;
  case 444:
#line 1984 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6533 "src/mca.tab.c"
    break;
  case 445:
#line 1985 "src/mca.y"
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6539 "src/mca.tab.c"
    break;
  case 446:
#line 1986 "src/mca.y"
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6545 "src/mca.tab.c"
    break;
  case 447:
#line 1987 "src/mca.y"
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
#line 6551 "src/mca.tab.c"
    break;
  case 448:
#line 1989 "src/mca.y"
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_DIV, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
#line 6560 "src/mca.tab.c"
    break;
  case 449:
#line 1994 "src/mca.y"
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_MUL, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
#line 6569 "src/mca.tab.c"
    break;
  case 450:
#line 1999 "src/mca.y"
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
	}
#line 6578 "src/mca.tab.c"
    break;
  case 451:
#line 2004 "src/mca.y"
                  {}
#line 6584 "src/mca.tab.c"
    break;
  case 452:
#line 2005 "src/mca.y"
                         {}
#line 6590 "src/mca.tab.c"
    break;
  case 453:
#line 2006 "src/mca.y"
                           {}
#line 6596 "src/mca.tab.c"
    break;
  case 454:
#line 2007 "src/mca.y"
                                  {}
#line 6602 "src/mca.tab.c"
    break;
#line 6606 "src/mca.tab.c"
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
#line 2009 "src/mca.y"
void mca_error(struct YYLTYPE *_loc, mc_value* mcast, const char *msg) {
    (void)_loc;
    (void)mcast;
    (void)msg;
    // All syntax errors are now caught by explicit error recovery rules
    // (E1002-E1031) with precise position spans and specific messages.
    // The generic E1000 fallback is no longer needed — silence GLR-internal
    // "syntax is ambiguous" noise and let the error rules do their job.
}
