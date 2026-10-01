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
  YYSYMBOL_MCK_IO = 70,                    /* MCK_IO  */
  YYSYMBOL_MCK_IN = 71,                    /* MCK_IN  */
  YYSYMBOL_MCK_OUT = 72,                   /* MCK_OUT  */
  YYSYMBOL_MCK_NC = 73,                    /* MCK_NC  */
  YYSYMBOL_MCK_PSRC = 74,                  /* MCK_PSRC  */
  YYSYMBOL_MCK_PSNK = 75,                  /* MCK_PSNK  */
  YYSYMBOL_MCK_PSBI = 76,                  /* MCK_PSBI  */
  YYSYMBOL_MCONST_HIGH = 77,               /* MCONST_HIGH  */
  YYSYMBOL_MCONST_LOW = 78,                /* MCONST_LOW  */
  YYSYMBOL_MCONST_NC = 79,                 /* MCONST_NC  */
  YYSYMBOL_MCU_INT = 80,                   /* MCU_INT  */
  YYSYMBOL_MCU_HEX = 81,                   /* MCU_HEX  */
  YYSYMBOL_MCU_FLOAT = 82,                 /* MCU_FLOAT  */
  YYSYMBOL_MCU_STRING = 83,                /* MCU_STRING  */
  YYSYMBOL_MCK_FUNC = 84,                  /* MCK_FUNC  */
  YYSYMBOL_MCK_THIS = 85,                  /* MCK_THIS  */
  YYSYMBOL_MCU_VOLT = 86,                  /* MCU_VOLT  */
  YYSYMBOL_MCU_AMP = 87,                   /* MCU_AMP  */
  YYSYMBOL_MCU_CAP = 88,                   /* MCU_CAP  */
  YYSYMBOL_MCU_IND = 89,                   /* MCU_IND  */
  YYSYMBOL_MCU_TIME = 90,                  /* MCU_TIME  */
  YYSYMBOL_MCU_LEN = 91,                   /* MCU_LEN  */
  YYSYMBOL_MCU_WATT = 92,                  /* MCU_WATT  */
  YYSYMBOL_MCU_OHM = 93,                   /* MCU_OHM  */
  YYSYMBOL_MCU_TEMP = 94,                  /* MCU_TEMP  */
  YYSYMBOL_MCU_HZ = 95,                    /* MCU_HZ  */
  YYSYMBOL_MCU_DB = 96,                    /* MCU_DB  */
  YYSYMBOL_MCU_PPM = 97,                   /* MCU_PPM  */
  YYSYMBOL_MCU_PERCENT = 98,               /* MCU_PERCENT  */
  YYSYMBOL_MCU_BAUD = 99,                  /* MCU_BAUD  */
  YYSYMBOL_MCU_DATASIZE = 100,             /* MCU_DATASIZE  */
  YYSYMBOL_MCU_SPS = 101,                  /* MCU_SPS  */
  YYSYMBOL_MCU_SIEMENS = 102,              /* MCU_SIEMENS  */
  YYSYMBOL_MCU_RESPONSIVITY = 103,         /* MCU_RESPONSIVITY  */
  YYSYMBOL_MCU_ANGLE = 104,                /* MCU_ANGLE  */
  YYSYMBOL_MCU_ANGULAR_RATE = 105,         /* MCU_ANGULAR_RATE  */
  YYSYMBOL_MCU_ENERGY = 106,               /* MCU_ENERGY  */
  YYSYMBOL_MCU_EFIELD = 107,               /* MCU_EFIELD  */
  YYSYMBOL_MCU_HFIELD = 108,               /* MCU_HFIELD  */
  YYSYMBOL_MCU_FLUX = 109,                 /* MCU_FLUX  */
  YYSYMBOL_MCU_BFIELD = 110,               /* MCU_BFIELD  */
  YYSYMBOL_MCU_SLEW = 111,                 /* MCU_SLEW  */
  YYSYMBOL_MCU_NOISE = 112,                /* MCU_NOISE  */
  YYSYMBOL_MCU_CHARGE = 113,               /* MCU_CHARGE  */
  YYSYMBOL_MCUVAL_VOLT = 114,              /* MCUVAL_VOLT  */
  YYSYMBOL_MCUVAL_AMP = 115,               /* MCUVAL_AMP  */
  YYSYMBOL_MCUVAL_CAP = 116,               /* MCUVAL_CAP  */
  YYSYMBOL_MCUVAL_IND = 117,               /* MCUVAL_IND  */
  YYSYMBOL_MCUVAL_TIME = 118,              /* MCUVAL_TIME  */
  YYSYMBOL_MCUVAL_LEN = 119,               /* MCUVAL_LEN  */
  YYSYMBOL_MCUVAL_WATT = 120,              /* MCUVAL_WATT  */
  YYSYMBOL_MCUVAL_OHM = 121,               /* MCUVAL_OHM  */
  YYSYMBOL_MCUVAL_TEMP = 122,              /* MCUVAL_TEMP  */
  YYSYMBOL_MCUVAL_HZ = 123,                /* MCUVAL_HZ  */
  YYSYMBOL_MCUVAL_DB = 124,                /* MCUVAL_DB  */
  YYSYMBOL_MCUVAL_PPM = 125,               /* MCUVAL_PPM  */
  YYSYMBOL_MCUVAL_PERCENT = 126,           /* MCUVAL_PERCENT  */
  YYSYMBOL_MCUVAL_BAUD = 127,              /* MCUVAL_BAUD  */
  YYSYMBOL_MCUVAL_DATASIZE = 128,          /* MCUVAL_DATASIZE  */
  YYSYMBOL_MCUVAL_SPS = 129,               /* MCUVAL_SPS  */
  YYSYMBOL_MCUVAL_SIEMENS = 130,           /* MCUVAL_SIEMENS  */
  YYSYMBOL_MCUVAL_RESPONSIVITY = 131,      /* MCUVAL_RESPONSIVITY  */
  YYSYMBOL_MCUVAL_ANGLE = 132,             /* MCUVAL_ANGLE  */
  YYSYMBOL_MCUVAL_ANGULAR_RATE = 133,      /* MCUVAL_ANGULAR_RATE  */
  YYSYMBOL_MCUVAL_ENERGY = 134,            /* MCUVAL_ENERGY  */
  YYSYMBOL_MCUVAL_EFIELD = 135,            /* MCUVAL_EFIELD  */
  YYSYMBOL_MCUVAL_HFIELD = 136,            /* MCUVAL_HFIELD  */
  YYSYMBOL_MCUVAL_FLUX = 137,              /* MCUVAL_FLUX  */
  YYSYMBOL_MCUVAL_BFIELD = 138,            /* MCUVAL_BFIELD  */
  YYSYMBOL_MCUVAL_SLEW = 139,              /* MCUVAL_SLEW  */
  YYSYMBOL_MCUVAL_NOISE = 140,             /* MCUVAL_NOISE  */
  YYSYMBOL_MCUVAL_CHARGE = 141,            /* MCUVAL_CHARGE  */
  YYSYMBOL_MCUVAL_COMPOSITE = 142,         /* MCUVAL_COMPOSITE  */
  YYSYMBOL_MC_WS = 143,                    /* MC_WS  */
  YYSYMBOL_MC_SINGLE_COMMENT = 144,        /* MC_SINGLE_COMMENT  */
  YYSYMBOL_MC_MULTI_COMMENT = 145,         /* MC_MULTI_COMMENT  */
  YYSYMBOL_UVALUE_WORD_PREC = 146,         /* UVALUE_WORD_PREC  */
  YYSYMBOL_IDA_BASE_PREC = 147,            /* IDA_BASE_PREC  */
  YYSYMBOL_YYACCEPT = 148,                 /* $accept  */
  YYSYMBOL_start = 149,                    /* start  */
  YYSYMBOL_mc_tops = 150,                  /* mc_tops  */
  YYSYMBOL_mc_top = 151,                   /* mc_top  */
  YYSYMBOL_mc_use = 152,                   /* mc_use  */
  YYSYMBOL_mc_uri = 153,                   /* mc_uri  */
  YYSYMBOL_mc_prefix = 154,                /* mc_prefix  */
  YYSYMBOL_mc_uri_trunk = 155,             /* mc_uri_trunk  */
  YYSYMBOL_mc_levels = 156,                /* mc_levels  */
  YYSYMBOL_mc_class_name = 157,            /* mc_class_name  */
  YYSYMBOL_mc_component = 158,             /* mc_component  */
  YYSYMBOL_mc_module = 159,                /* mc_module  */
  YYSYMBOL_mc_interface = 160,             /* mc_interface  */
  YYSYMBOL_mc_enum = 161,                  /* mc_enum  */
  YYSYMBOL_mc_bom = 162,                   /* mc_bom  */
  YYSYMBOL_mc_recipe = 163,                /* mc_recipe  */
  YYSYMBOL_mc_component_derivs = 164,      /* mc_component_derivs  */
  YYSYMBOL_mc_component_variant = 165,     /* mc_component_variant  */
  YYSYMBOL_mc_component_adopts = 166,      /* mc_component_adopts  */
  YYSYMBOL_mc_body = 167,                  /* mc_body  */
  YYSYMBOL_mc_clauses = 168,               /* mc_clauses  */
  YYSYMBOL_mc_clause = 169,                /* mc_clause  */
  YYSYMBOL_mc_attribute = 170,             /* mc_attribute  */
  YYSYMBOL_mc_attr_values = 171,           /* mc_attr_values  */
  YYSYMBOL_mc_attr_value = 172,            /* mc_attr_value  */
  YYSYMBOL_mc_attr_lines = 173,            /* mc_attr_lines  */
  YYSYMBOL_mc_attribute_pin = 174,         /* mc_attribute_pin  */
  YYSYMBOL_mc_pins_lines = 175,            /* mc_pins_lines  */
  YYSYMBOL_mc_pins_line = 176,             /* mc_pins_line  */
  YYSYMBOL_mc_pin_idn = 177,               /* mc_pin_idn  */
  YYSYMBOL_mc_pins_names = 178,            /* mc_pins_names  */
  YYSYMBOL_mc_pins_name = 179,             /* mc_pins_name  */
  YYSYMBOL_mc_nc_list = 180,               /* mc_nc_list  */
  YYSYMBOL_mc_net = 181,                   /* mc_net  */
  YYSYMBOL_mc_opds = 182,                  /* mc_opds  */
  YYSYMBOL_mc_net_port_elem = 183,         /* mc_net_port_elem  */
  YYSYMBOL_mc_net_port_elems = 184,        /* mc_net_port_elems  */
  YYSYMBOL_mc_mn_opd = 185,                /* mc_mn_opd  */
  YYSYMBOL_mc_mn_opds = 186,               /* mc_mn_opds  */
  YYSYMBOL_mc_opd = 187,                   /* mc_opd  */
  YYSYMBOL_mc_phrases = 188,               /* mc_phrases  */
  YYSYMBOL_mc_list_items = 189,            /* mc_list_items  */
  YYSYMBOL_mc_list_sep = 190,              /* mc_list_sep  */
  YYSYMBOL_mc_list_item = 191,             /* mc_list_item  */
  YYSYMBOL_mc_phrase = 192,                /* mc_phrase  */
  YYSYMBOL_mc_role = 193,                  /* mc_role  */
  YYSYMBOL_mc_conduit = 194,               /* mc_conduit  */
  YYSYMBOL_mc_domain = 195,                /* mc_domain  */
  YYSYMBOL_mc_rail = 196,                  /* mc_rail  */
  YYSYMBOL_mc_partition = 197,             /* mc_partition  */
  YYSYMBOL_mc_error_stmt = 198,            /* mc_error_stmt  */
  YYSYMBOL_mc_error_msg = 199,             /* mc_error_msg  */
  YYSYMBOL_mc_tattrs = 200,                /* mc_tattrs  */
  YYSYMBOL_mc_tattrs_opt = 201,            /* mc_tattrs_opt  */
  YYSYMBOL_mc_tattr = 202,                 /* mc_tattr  */
  YYSYMBOL_mc_tattr_args = 203,            /* mc_tattr_args  */
  YYSYMBOL_mc_tattr_arg = 204,             /* mc_tattr_arg  */
  YYSYMBOL_mc_tkey = 205,                  /* mc_tkey  */
  YYSYMBOL_mc_resword = 206,               /* mc_resword  */
  YYSYMBOL_mc_function = 207,              /* mc_function  */
  YYSYMBOL_mc_paramds = 208,               /* mc_paramds  */
  YYSYMBOL_mc_pards = 209,                 /* mc_pards  */
  YYSYMBOL_mc_pard = 210,                  /* mc_pard  */
  YYSYMBOL_mc_declare_a = 211,             /* mc_declare_a  */
  YYSYMBOL_mc_declare_a1 = 212,            /* mc_declare_a1  */
  YYSYMBOL_mc_insts = 213,                 /* mc_insts  */
  YYSYMBOL_mc_inst = 214,                  /* mc_inst  */
  YYSYMBOL_mc_iface_name = 215,            /* mc_iface_name  */
  YYSYMBOL_mc_declare_b = 216,             /* mc_declare_b  */
  YYSYMBOL_mc_params = 217,                /* mc_params  */
  YYSYMBOL_mc_body_params = 218,           /* mc_body_params  */
  YYSYMBOL_mc_param = 219,                 /* mc_param  */
  YYSYMBOL_mc_conds = 220,                 /* mc_conds  */
  YYSYMBOL_mc_conds_elifs = 221,           /* mc_conds_elifs  */
  YYSYMBOL_mc_cond_block = 222,            /* mc_cond_block  */
  YYSYMBOL_mc_expr = 223,                  /* mc_expr  */
  YYSYMBOL_mc_judge = 224,                 /* mc_judge  */
  YYSYMBOL_mc_id = 225,                    /* mc_id  */
  YYSYMBOL_mc_ida = 226,                   /* mc_ida  */
  YYSYMBOL_mc_ida_tail = 227,              /* mc_ida_tail  */
  YYSYMBOL_mc_ida_bracket = 228,           /* mc_ida_bracket  */
  YYSYMBOL_mc_ida_items = 229,             /* mc_ida_items  */
  YYSYMBOL_mc_ida_item = 230,              /* mc_ida_item  */
  YYSYMBOL_mc_ida_run = 231,               /* mc_ida_run  */
  YYSYMBOL_mc_ida_run_atom = 232,          /* mc_ida_run_atom  */
  YYSYMBOL_mc_idss = 233,                  /* mc_idss  */
  YYSYMBOL_mc_ids = 234,                   /* mc_ids  */
  YYSYMBOL_mc_idseg = 235,                 /* mc_idseg  */
  YYSYMBOL_mc_idm = 236,                   /* mc_idm  */
  YYSYMBOL_mc_idans = 237,                 /* mc_idans  */
  YYSYMBOL_mc_idan = 238,                  /* mc_idan  */
  YYSYMBOL_mc_int = 239,                   /* mc_int  */
  YYSYMBOL_mc_hex = 240,                   /* mc_hex  */
  YYSYMBOL_mc_float = 241,                 /* mc_float  */
  YYSYMBOL_mc_number = 242,                /* mc_number  */
  YYSYMBOL_mc_string = 243,                /* mc_string  */
  YYSYMBOL_mc_const = 244,                 /* mc_const  */
  YYSYMBOL_mc_nc = 245,                    /* mc_nc  */
  YYSYMBOL_mc_underscore = 246,            /* mc_underscore  */
  YYSYMBOL_mc_literal = 247,               /* mc_literal  */
  YYSYMBOL_mc_cond_word = 248,             /* mc_cond_word  */
  YYSYMBOL_mc_iotype = 249,                /* mc_iotype  */
  YYSYMBOL_mc_unit_value = 250,            /* mc_unit_value  */
  YYSYMBOL_mc_unit_type = 251,             /* mc_unit_type  */
  YYSYMBOL_mc_endls = 252                  /* mc_endls  */
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
#define YYLAST   5227
/* YYNTOKENS -- Number of terminals.  */
#define YYNTOKENS  148
/* YYNNTS -- Number of nonterminals.  */
#define YYNNTS  105
/* YYNRULES -- Number of rules.  */
#define YYNRULES  593
/* YYNSTATES -- Number of states.  */
#define YYNSTATES  942
/* YYMAXRHS -- Maximum number of symbols on right-hand side of rule.  */
#define YYMAXRHS 9
/* YYMAXLEFT -- Maximum number of symbols to the left of a handle
   accessed by $0, $-1, etc., in any rule.  */
#define YYMAXLEFT 0
/* YYMAXUTOK -- Last valid token kind.  */
#define YYMAXUTOK   402
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
     145,   146,   147
};
#if MCA_DEBUG
/* YYRLINE[YYN] -- source line where rule number YYN was defined.  */
static const yytype_int16 yyrline[] =
{
       0,   290,   290,   291,   292,   293,   294,   296,   297,   299,
     300,   301,   302,   303,   304,   305,   306,   314,   319,   333,
     338,   357,   365,   375,   381,   389,   393,   403,   404,   405,
     407,   411,   416,   421,   426,   432,   443,   459,   460,   461,
     462,   463,   464,   468,   469,   478,   489,   503,   513,   523,
     535,   541,   549,   552,   554,   556,   558,   561,   574,   575,
     576,   578,   579,   580,   581,   582,   583,   584,   585,   586,
     587,   588,   589,   592,   611,   628,   643,   654,   659,   666,
     670,   674,   678,   683,   694,   695,   699,   709,   719,   732,
     733,   734,   736,   743,   751,   759,   769,   781,   790,   801,
     812,   822,   827,   832,   837,   842,   849,   859,   865,   870,
     875,   882,   883,   888,   893,   898,   903,   912,   925,   926,
     940,   941,   942,   943,   959,   960,   961,   966,   967,   970,
     975,   980,   986,   992,  1000,  1012,  1018,  1024,  1032,  1041,
    1046,  1053,  1058,  1063,  1068,  1069,  1070,  1084,  1089,  1095,
    1099,  1104,  1105,  1106,  1116,  1129,  1130,  1132,  1133,  1134,
    1135,  1137,  1138,  1139,  1140,  1141,  1142,  1143,  1144,  1145,
    1147,  1148,  1149,  1150,  1151,  1152,  1153,  1154,  1155,  1157,
    1158,  1159,  1160,  1161,  1162,  1163,  1164,  1165,  1174,  1175,
    1176,  1178,  1179,  1180,  1181,  1182,  1183,  1184,  1185,  1186,
    1188,  1189,  1190,  1191,  1192,  1193,  1194,  1195,  1196,  1198,
    1199,  1200,  1201,  1202,  1203,  1204,  1205,  1206,  1208,  1209,
    1210,  1211,  1213,  1214,  1215,  1216,  1218,  1223,  1228,  1233,
    1252,  1272,  1283,  1291,  1311,  1343,  1348,  1354,  1359,  1366,
    1374,  1384,  1394,  1403,  1412,  1420,  1428,  1436,  1446,  1455,
    1466,  1477,  1487,  1496,  1505,  1514,  1548,  1555,  1564,  1571,
    1582,  1587,  1595,  1609,  1615,  1621,  1638,  1655,  1665,  1669,
    1673,  1677,  1681,  1685,  1698,  1703,  1710,  1713,  1718,  1723,
    1740,  1749,  1754,  1760,  1771,  1775,  1779,  1791,  1791,  1791,
    1791,  1791,  1791,  1791,  1793,  1805,  1817,  1818,  1819,  1821,
    1822,  1826,  1830,  1838,  1844,  1851,  1858,  1864,  1870,  1876,
    1882,  1888,  1898,  1907,  1923,  1935,  1943,  1952,  1960,  1968,
    1977,  1982,  1989,  1994,  2004,  2005,  2006,  2013,  2021,  2031,
    2032,  2033,  2045,  2046,  2047,  2048,  2051,  2055,  2059,  2063,
    2071,  2078,  2094,  2107,  2116,  2128,  2140,  2145,  2154,  2159,
    2171,  2176,  2183,  2188,  2193,  2200,  2201,  2202,  2204,  2205,
    2206,  2207,  2208,  2209,  2210,  2211,  2212,  2213,  2214,  2215,
    2216,  2227,  2235,  2244,  2245,  2252,  2253,  2254,  2255,  2260,
    2261,  2262,  2263,  2267,  2273,  2273,  2273,  2274,  2274,  2274,
    2274,  2274,  2274,  2274,  2274,  2275,  2275,  2275,  2275,  2275,
    2275,  2275,  2275,  2276,  2276,  2276,  2276,  2276,  2276,  2277,
    2277,  2277,  2277,  2277,  2277,  2277,  2277,  2277,  2278,  2278,
    2278,  2279,  2279,  2279,  2279,  2280,  2280,  2280,  2280,  2280,
    2280,  2280,  2280,  2280,  2280,  2281,  2281,  2281,  2281,  2281,
    2281,  2281,  2281,  2282,  2282,  2282,  2282,  2282,  2282,  2282,
    2283,  2283,  2283,  2284,  2284,  2284,  2284,  2284,  2284,  2284,
    2284,  2285,  2285,  2285,  2285,  2285,  2285,  2285,  2286,  2286,
    2286,  2286,  2286,  2286,  2287,  2287,  2287,  2287,  2287,  2287,
    2287,  2287,  2290,  2291,  2292,  2295,  2296,  2297,  2298,  2299,
    2300,  2301,  2302,  2303,  2304,  2305,  2306,  2307,  2309,  2310,
    2311,  2312,  2313,  2314,  2316,  2318,  2319,  2320,  2321,  2326,
    2327,  2328,  2329,  2333,  2347,  2351,  2355,  2364,  2368,  2386,
    2387,  2388,  2389,  2390,  2391,  2392,  2399,  2400,  2401,  2402,
    2403,  2404,  2405,  2406,  2407,  2408,  2409,  2410,  2411,  2412,
    2413,  2414,  2415,  2416,  2417,  2418,  2419,  2420,  2421,  2422,
    2423,  2424,  2425,  2426,  2427,  2431,  2432,  2433,  2434,  2435,
    2436,  2437,  2438,  2439,  2440,  2441,  2442,  2443,  2444,  2445,
    2446,  2447,  2448,  2449,  2450,  2451,  2452,  2453,  2454,  2455,
    2456,  2457,  2458,  2459,  2460,  2461,  2462,  2463,  2468,  2473,
    2479,  2480,  2481,  2482
};
#endif
#define YYPACT_NINF (-772)
#define YYTABLE_NINF (-496)
/* YYPACT[STATE-NUM] -- Index in YYTABLE of the portion describing
   STATE-NUM.  */
static const yytype_int16 yypact[] =
{
     544,  -772,    52,    99,   394,   394,   394,   394,   394,    26,
    -772,   394,  -772,    56,   145,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,   680,   149,  -772,  -772,   138,  -772,   100,
     113,   115,   626,  -772,    91,    81,  -772,  -772,   151,   306,
      81,    81,   171,   205,   394,   205,  -772,   860,  -772,  -772,
     145,  -772,   239,   264,  -772,   394,   322,   394,  -772,   113,
     642,  -772,   113,   113,   113,   113,  3629,    64,  -772,   659,
     381,   448,   394,   625,  -772,   205,   205,   394,  2921,  -772,
      81,  -772,  -772,   964,   394,  -772,   348,  -772,   394,  -772,
    -772,   477,   491,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,   165,  -772,   507,  -772,    91,
    -772,  -772,   492,   363,  4609,   178,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,   496,   -17,   104,  -772,  -772,  -772,
     166,  -772,  4609,   394,   205,   471,  -772,  -772,  -772,   607,
      96,  -772,   540,  -772,  -772,   458,  -772,  -772,  -772,  -772,
      69,   394,   394,   394,  4609,   394,   582,  4609,  4609,  1175,
    4749,  4609,   515,  -772,  -772,   113,   623,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,    89,  -772,  -772,  -772,
    -772,  1856,  1121,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,   562,   565,  -772,   561,   253,  -772,  -772,  -772,  -772,
    -772,  -772,   533,  4609,   598,   381,   348,   394,  -772,  -772,
    -772,  3629,  -772,  3629,  -772,   394,   625,   583,   634,  1856,
     247,  2103,    83,   533,   577,   644,   625,   667,  4049,   394,
     758,  -772,   448,  1080,  4049,  5114,  -772,   592,  2103,   222,
    -772,  -772,   394,  -772,  -772,  -772,  -772,   625,   448,  -772,
    4609,  -772,   457,  -772,  -772,   652,   669,   669,  2103,   205,
     625,    69,   632,   654,  1371,   287,   240,  -772,  1498,   670,
     452,  1548,  1047,  -772,  -772,  4749,   957,  2149,   798,  2779,
    1790,   663,  2390,    81,    69,  -772,  3205,  4609,   113,    69,
      87,  4609,  4609,  4609,  4609,  4609,  4609,  -772,  -772,  4609,
     448,  4609,   625,   102,  4889,   124,  4609,  4609,  4609,  4609,
    4609,  4609,  -772,  -772,  4609,   448,   669,  -772,  -772,   625,
     625,  4049,   448,  4189,  4189,  4049,   685,   692,  4609,  4609,
    4609,  4609,  4609,  4609,   448,  -772,    31,  1856,  1121,   466,
    1604,   205,  -772,  -772,  -772,  -772,  -772,   448,    69,  4609,
     657,  -772,   448,  -772,   448,  -772,   661,  -772,  1856,  2103,
     223,  -772,   357,  -772,   533,   679,   712,  -772,   746,  -772,
    -772,   243,  5114,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,  -772,  -772,   538,  4049,   348,  -772,  -772,
    -772,   291,  -772,    69,    69,  2921,  -772,   205,  -772,   726,
     422,   439,   630,   590,  2599,  2599,   657,  -772,  -772,  4609,
     175,  4049,  4609,  4609,  -772,  1311,  1047,   451,   707,  4609,
    4609,  4609,  4609,  4609,  4609,  4609,  4609,  4609,  3063,  4749,
    4749,  -772,  -772,   631,  3205,   719,   714,   724,   205,   486,
    -772,   747,   852,   257,   723,   502,   178,  4609,  -772,   397,
     607,   607,   608,   364,   608,   364,   608,   364,    32,   608,
     364,    32,   277,   527,  -772,   277,   527,  -772,  1918,   992,
     514,  -772,   747,   852,   257,   732,  -772,  -772,  -772,  -772,
     733,  -772,  4609,   509,  1856,  2005,   505,   178,  -772,   574,
     607,   607,   608,   364,   608,   364,   608,   364,    32,   608,
     364,    32,   277,   527,  -772,   277,   527,  -772,  1918,   992,
     514,  -772,  -772,   684,   699,   263,   429,   717,  5029,   755,
    -772,  1856,  2103,   533,   755,   266,   394,   394,   747,   852,
     257,   608,   364,    32,   608,   364,    32,   277,   527,  -772,
     277,   527,  -772,  1918,   992,   514,  -772,    93,  -772,  -772,
    -772,   727,  -772,  -772,  -772,  1856,  2103,   533,   394,  -772,
    -772,    69,  4049,   721,  4189,  4189,  4049,   448,  -772,   394,
     549,  4609,  5114,  5114,   267,  -772,  -772,   713,   197,  -772,
     736,    69,   754,  -772,   159,  -772,   761,  2103,   762,   763,
    4609,   228,  1856,  -772,  2103,   533,   325,   336,   340,  -772,
    4609,  2040,  -772,   533,  -772,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -772,   756,  4749,  3347,   638,  -772,  2390,  -772,
    2390,  -772,    69,  4049,    69,   551,  -772,  4049,  -772,  -772,
    4049,  3769,   382,    69,  -772,   556,  4049,  -772,  -772,  -772,
    -772,  -772,  4189,   685,   692,  -772,   662,  -772,   282,   372,
    4189,   394,  -772,  -772,  -772,   -17,  -772,   113,   740,   715,
    -772,   394,   669,   669,   286,  -772,  -772,  -772,  -772,  2103,
     533,  -772,  -772,  -772,  -772,  -772,  2599,   716,  4189,  -772,
    3489,  4329,   770,  -772,   669,  -772,  -772,   393,  2779,  -772,
    4749,  3347,  -772,  -772,  -772,  -772,   728,   362,   730,   178,
     205,   374,   385,  -772,  -772,  -772,  -772,  -772,  -772,  -772,
     387,   398,  -772,   749,    90,   657,   748,   205,   409,  -772,
     755,  -772,   109,   448,  -772,   685,   692,  -772,  4049,   773,
     753,   669,   669,  4049,  -772,  3909,   307,  -772,   755,  -772,
    4469,    27,  -772,  1856,  2103,   249,  -772,  4329,   669,  -772,
    -772,  2779,  -772,  -772,  -772,  -772,  -772,  -772,   721,  -772,
    -772,   113,  -772,  -772,  4189,  -772,  -772,   721,   790,  -772,
     573,   429,   405,  4189,  4049,  -772,  -772,  -772,   420,  -772,
    4189,  4329,   201,   198,  -772,  -772,  -772,  -772,   779,  -772,
    -772,   625,   448,  -772,  -772,   408,   739,  -772,   755,  -772,
    4189,  4189,   232,   760,   785,  -772,  -772,   755,   755,  4189,
     754,   755
};
/* YYDEFACT[STATE-NUM] -- Default reduction number in state STATE-NUM.
   Performed when YYTABLE does not specify something else to do.  Zero
   means the default is an error.  */
static const yytype_int16 yydefact[] =
{
       0,    16,     0,     0,     0,     0,     0,     0,     0,     0,
     591,     0,   590,     0,     2,     8,     9,    10,    11,    12,
      13,    14,    15,     0,     0,    21,   372,     0,    27,    19,
       0,    23,     0,    42,     0,   298,   373,   486,    44,   485,
     298,   298,     0,     0,     0,     0,     1,     0,   593,   592,
       4,    22,    17,     0,    28,     0,    25,     0,    36,     0,
       0,    33,     0,     0,     0,     0,     0,   374,   375,     0,
      54,     0,     0,     0,   487,     0,     0,     0,     0,    51,
     298,    50,     7,     0,     0,    29,    20,   484,     0,    24,
      37,    32,    31,    40,    39,    41,    38,   385,   386,   384,
     387,   388,   389,   390,   391,   392,   393,   394,   395,   396,
     397,   398,   399,   400,   401,   402,   403,   405,   406,   404,
     407,   408,   409,   410,   411,   412,   413,   414,   415,   418,
     419,   420,   421,   422,   423,   424,   416,   417,   425,   426,
     427,   428,   429,   430,   431,   432,   433,   434,   435,   436,
     437,   438,   439,   440,   441,   442,   443,   444,   445,   446,
     447,   448,   449,   450,   451,   452,   453,   454,   455,   456,
     457,   458,   459,   460,   461,   462,   463,   464,   465,   466,
     467,   468,   469,   470,   471,   472,   473,   474,   475,   476,
     477,   478,   479,   480,   481,     0,   379,   381,   383,     0,
     376,   508,   301,   135,     0,     0,   297,   521,   519,   520,
     525,   522,   523,   524,   131,     0,     0,   300,   309,   310,
     303,   139,     0,     0,     0,    56,   498,    43,   488,   494,
       0,   492,   495,    47,    48,     0,    72,   499,   500,   504,
       0,     0,     0,     0,     0,     0,   135,     0,     0,     0,
       0,     0,     0,   505,   506,     0,   131,   526,   527,   528,
     529,   530,   531,   532,   533,   534,   535,   536,   537,   538,
     539,   540,   541,   542,   543,   544,   545,   546,   547,   548,
     549,   550,   551,   552,   553,   554,     0,    59,    61,    62,
      63,     0,   276,    64,    67,    68,    69,    70,    71,    65,
     115,   155,   156,    66,   373,   129,   501,   502,   503,   509,
     510,   511,     0,     0,   512,    54,    18,   483,    26,    35,
      34,     0,   378,     0,   377,     0,     0,   136,   135,   145,
       0,   144,   129,   146,     0,   129,     0,   132,   331,     0,
       0,   296,     0,     0,   331,     0,   317,   322,   307,   129,
      53,    45,     0,    52,   489,   497,   490,     0,     0,    49,
       0,   128,     0,   124,   125,     0,   276,   276,   265,     0,
       0,     0,     0,     0,   145,     0,     0,   148,   144,   373,
     146,     0,     0,   516,   515,     0,   356,   357,     0,     0,
     371,   117,     0,   298,     0,    57,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   159,   157,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   160,   158,     0,     0,   277,   116,   274,     0,
       0,   331,     0,     0,     0,   331,   314,   317,     0,     0,
       0,     0,     0,     0,     0,   123,   276,   120,   276,   156,
       0,     0,   482,   380,   382,   302,   137,     0,     0,     0,
     306,   140,     0,   133,     0,   338,   135,   507,   339,   340,
       0,   330,   129,   337,   336,     0,   324,   299,   130,   304,
     305,     0,     0,   555,   556,   557,   558,   559,   560,   561,
     562,   563,   569,   570,   564,   565,   566,   567,   568,   571,
     572,   573,   574,   575,   576,   577,   578,   579,   580,   581,
     582,   583,   584,   585,   586,   311,   331,    55,   491,   493,
     496,     0,   233,     0,     0,     0,   263,     0,   266,   373,
       0,   486,     0,   125,     0,     0,     0,   149,   237,     0,
     150,   331,     0,     0,   238,   356,   357,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   354,   352,   346,     0,   269,     0,   268,     0,     0,
      58,   209,   215,   210,     0,     0,     0,     0,   256,     0,
     244,   246,   222,   224,   218,   220,   161,   167,   162,   170,
     176,   171,   179,   185,   180,   191,   197,   192,   200,   206,
     201,   188,   213,   217,   214,   373,   227,   226,   285,   286,
     278,   284,     0,     0,   124,     0,   501,     0,   258,     0,
     245,   247,   223,   225,   219,   221,   165,   169,   166,   174,
     178,   175,   183,   187,   184,   195,   199,   196,   204,   208,
     205,   190,   275,     0,     0,     0,   130,     0,     0,    73,
      78,    80,    81,    79,    75,     0,     0,     0,   211,   216,
     212,   163,   168,   164,   172,   177,   173,   181,   186,   182,
     193,   198,   194,   202,   207,   203,   189,     0,   113,   114,
     517,   514,   513,    46,   138,   143,   141,   142,     0,   130,
     134,     0,     0,     0,     0,     0,   335,     0,   318,     0,
       0,     0,     0,     0,     0,   126,   127,     0,     0,   264,
       0,     0,   490,   100,     0,    90,     0,   103,   486,   501,
       0,     0,   152,   147,   151,   153,     0,     0,     0,   370,
       0,   356,   358,   355,   361,   362,   363,   364,   365,   366,
     367,   368,   359,   360,     0,     0,   348,   353,     0,   267,
       0,   295,     0,   331,     0,     0,   119,   331,   252,   254,
     331,     0,     0,     0,   228,     0,   331,   253,   255,   235,
     236,   239,     0,   315,   318,    83,   135,    85,     0,   129,
       0,     0,   320,   321,   122,   120,   121,     0,     0,     0,
     329,     0,   343,   341,     0,   334,   325,   319,   589,   313,
     312,   588,   587,   323,   234,   262,     0,     0,     0,    86,
       0,     0,     0,    87,   239,   260,   261,     0,     0,   347,
       0,     0,   270,   271,   272,   273,     0,     0,     0,     0,
       0,     0,     0,   289,   287,   288,   290,   291,   292,   293,
       0,     0,   282,     0,   373,   126,     0,     0,     0,   248,
      76,    82,     0,     0,    77,   316,   319,   518,   335,   490,
       0,   344,   342,     0,   326,     0,     0,   230,    74,    89,
       0,    92,   105,   109,   110,   501,   106,     0,   154,   369,
     350,     0,   349,   229,   240,   231,   118,   257,   242,   241,
     279,     0,   280,   294,     0,   232,   259,   243,     0,    84,
       0,   130,     0,     0,   335,   332,   333,    88,     0,   112,
       0,     0,    96,    94,   351,   249,   250,   281,     0,   283,
     251,     0,     0,   328,   345,     0,     0,   107,    93,   104,
       0,     0,    97,     0,     0,   327,   111,    98,    95,     0,
       0,    99
};
/* YYPGOTO[NTERM-NUM].  */
static const yytype_int16 yypgoto[] =
{
    -772,  -772,   799,   132,  -772,   797,  -772,   793,  -772,    82,
    -772,  -772,  -772,  -772,  -772,  -772,   510,  -772,  -772,   -27,
     299,   -67,  -633,  -415,  -643,  -772,  -772,  -515,    16,   121,
     -35,   -66,  -772,  -772,   227,   172,  -772,   328,  -204,   711,
    -183,  -772,  -772,   313,  1258,  -772,  -772,  -772,  -772,  -772,
    -772,  -772,  -755,  -286,  -409,  -772,   -38,  -772,  -772,  -772,
     -11,  -772,   516,  -772,   -44,  -612,  -295,  -649,   -57,  -270,
    -720,  -669,  -772,  -772,  -713,   635,  -361,    -2,  1643,  -772,
     -32,  -772,   534,   531,  -772,   -51,   200,  -772,    44,  -364,
     500,  1463,  -772,  -772,   613,  -772,  -772,  -771,  -772,  1887,
    -772,   -61,  -218,  -469,     0
};
/* YYDEFGOTO[NTERM-NUM].  */
static const yytype_int16 yydefgoto[] =
{
       0,    13,    14,    15,    16,    29,    30,    31,    32,    35,
      17,    18,    19,    20,    21,    22,   224,   225,   353,   561,
     286,   562,   288,   649,   650,   778,   289,   714,   715,   716,
     871,   872,   908,   290,   755,   445,   446,   361,   530,   291,
     375,   376,   539,   377,   469,   293,   294,   295,   296,   297,
     298,   566,   426,   427,   428,   841,   842,   610,   843,   299,
      70,   216,   217,   300,   301,   436,   346,   475,   302,   470,
     794,   471,   303,   746,   563,   388,   389,   304,    37,    67,
      68,   195,   196,   197,   198,    86,   332,    39,   327,   230,
     231,   306,   307,   308,   309,   310,   311,   473,   221,   312,
     681,   313,   314,   515,   564
};
/* YYTABLE[YYPACT[STATE-NUM]] -- What to do in state STATE-NUM.  If
   positive, shift that token.  If negative, reduce the rule whose
   number is the opposite.  If YYTABLE_NINF, syntax error.  */
static const yytype_int16 yytable[] =
{
      23,    33,    36,    36,    36,    36,    36,   532,   222,    36,
     437,   287,   219,   700,    47,   777,    79,   642,    81,   654,
     721,   330,    33,   790,   547,   218,   235,   795,    33,    75,
      76,   384,   819,   316,   773,   200,   362,   861,   862,   788,
     876,   338,    36,   413,    44,   438,   339,   413,   233,   234,
      83,   792,   793,    36,   910,    36,    46,    90,   677,   878,
      93,    94,    95,    96,    24,   643,   644,    36,   391,   315,
      36,    36,   226,   199,   481,    36,   911,    26,    34,   201,
     526,   527,    36,    74,   441,   442,    36,    40,    41,    42,
      43,    26,    34,    45,   444,    26,    34,   201,   342,   909,
      25,    26,    34,   201,   203,   880,   876,    26,   882,    78,
      26,    34,   395,    55,    27,    10,   912,    26,    34,   356,
      66,    26,   203,   357,   894,   360,    80,   205,   203,    57,
     608,   340,    26,    34,   201,    48,   576,   854,   902,    69,
     876,   344,   860,   577,   898,   205,    78,    66,   431,   577,
      51,   205,    28,    53,   214,   936,    12,    26,   932,   203,
     678,   645,   679,   341,    27,   655,    71,   324,   914,   855,
     609,    10,   214,   617,    26,    34,    49,   521,   214,    82,
     577,   342,   205,   698,   925,    10,    26,    34,   201,   795,
     569,    54,   321,    77,   905,   575,   906,   351,   742,   743,
     343,    48,    28,    36,    38,    38,    38,    38,    38,   214,
     613,    38,    12,   203,   413,    82,   809,   413,    36,   899,
     805,    36,   322,    10,   344,   931,    12,    78,   930,   345,
      26,    34,   682,   801,   802,   795,   205,   342,    36,    36,
      36,    36,    49,    36,    38,   379,   704,   911,   413,  -308,
     692,   919,    84,   393,    10,    87,   449,    89,   337,   939,
     924,    26,    34,   214,    12,  -108,    10,   537,   432,   220,
     692,   726,   354,   355,   459,  -108,  -108,    87,   305,   222,
     344,  -308,   693,   219,    87,   813,   396,   433,   318,   434,
     692,   866,   398,   692,   692,    12,   218,   538,  -108,   399,
     337,   517,   699,    36,   460,   350,  -108,    12,    10,   441,
     442,   435,    10,   863,   459,    36,  -108,    85,   459,   444,
     707,    72,   771,    36,    36,   781,   803,   789,    73,   570,
      36,   407,   408,    10,    36,   338,    88,    36,    36,   851,
     339,    36,   528,   581,   536,   864,   840,    36,   705,    12,
      36,   774,   692,    12,   362,    36,   580,   850,   621,   727,
     728,   782,   783,   459,   907,    26,    34,   459,   529,    36,
     694,   620,   342,   578,    12,   317,   540,   411,   326,   412,
      26,    34,   568,   818,   814,    73,   414,   853,   618,   692,
      36,   695,    36,   868,   223,   815,   574,    36,    36,   816,
     518,   692,    26,    34,   797,   335,   433,   415,   434,   459,
     605,    36,   692,    36,   780,   344,   420,   421,   422,   423,
     459,   884,   349,    38,   683,   891,   425,    36,    36,   762,
     344,    10,   863,   888,    10,   863,   692,    26,    34,   845,
     335,   365,   366,   367,   889,   369,   890,   926,   680,   523,
     879,   226,   642,   642,    72,   757,    36,   892,   287,   881,
     339,    73,  -494,   772,   923,   438,  -486,   935,   897,   642,
      36,   711,    12,   720,   720,    12,    36,   927,  -153,  -146,
     522,   359,  -121,   827,   523,   317,   856,   831,   430,  -121,
     832,   287,  -121,  -121,   319,   928,   848,   747,   439,   440,
     709,   559,   560,   642,   441,   442,   524,   807,   320,  -146,
     729,   336,   443,   523,   444,   937,   938,   452,    73,  -153,
     323,    36,    36,   642,   941,   455,   325,   438,  -125,   523,
    -121,  -121,  -125,  -121,   352,   752,   523,   379,   472,   476,
     220,   751,   412,   479,   472,     1,   438,   817,   826,   414,
     828,   754,    87,   358,  -125,     2,     3,   933,   763,   846,
     439,   440,     4,     5,     6,     7,   441,   442,     8,     9,
      10,   335,   701,   392,    36,   355,   444,    11,   829,   439,
     440,   422,   423,   829,   429,   441,   442,   430,   922,   305,
     702,   703,   335,   443,   335,   444,   305,   370,   457,   335,
     830,   702,   703,   358,   371,   847,   698,   433,   798,   434,
     379,    12,   606,  -495,   450,    36,   372,  -125,   373,   431,
     786,   397,    72,   398,   758,   759,   -30,    58,   226,    73,
     399,   472,   766,    26,    34,   472,   461,   339,   336,   -30,
     -30,    59,    60,    61,    36,   394,   379,    91,    92,   326,
     516,   400,   -30,   712,    36,    36,   458,   357,   335,   462,
     405,   406,   407,   408,   767,   768,   338,    26,    34,   201,
     410,   339,    62,    63,   525,    36,   326,   326,    64,    65,
      -6,     1,   464,   691,   371,   413,    36,   202,   534,    36,
     459,     2,     3,   -30,   203,   744,   745,    36,     4,     5,
       6,     7,   820,   821,     8,     9,    48,   769,   396,    36,
     535,   357,   656,    11,   810,   204,   472,   205,   206,   657,
     688,   810,   770,   335,   335,   305,   357,   697,   541,   207,
     208,   209,   210,   211,   212,   213,   804,   696,   859,   867,
     523,   472,   357,   523,   214,   720,    36,    49,    36,   720,
      36,   883,    36,   885,    26,   523,   518,   523,   305,   844,
     710,    36,   398,   730,   305,   748,    26,    34,   201,   399,
     750,   895,   775,   749,   787,   523,   335,    36,   852,    36,
     215,   753,   780,   940,   791,   857,   202,   357,   808,    36,
     760,   761,   806,   203,   865,   811,  -102,  -101,   858,   405,
     406,   407,   408,   887,   877,   338,   559,   903,   893,   410,
     339,   904,   921,   894,   204,   849,   205,   335,   467,   772,
     896,    52,    50,    56,   708,   451,   869,    36,   207,   208,
     209,   210,   211,   212,   213,   549,   550,   551,   552,   553,
     554,   812,   913,   214,   765,   929,   555,   556,   779,   784,
      36,   706,   723,   917,   454,   453,   477,   519,   849,   557,
      -3,     1,   383,     0,     0,     0,   810,   412,   379,     0,
       0,     2,     3,     0,   414,     0,     0,   335,     4,     5,
       6,     7,     0,     0,     8,     9,    48,     0,   476,   918,
       0,   335,   472,    11,     0,     0,   472,     0,     0,    36,
       0,     0,   865,   518,   420,   421,   422,   423,     0,     0,
       0,   335,     0,     0,   425,   329,   334,     0,     0,    36,
       0,     0,     0,     0,     0,   865,     0,    49,   915,     0,
       0,     0,     0,   916,     0,     0,     0,     0,     0,     0,
       0,   920,     0,     0,     0,   305,     0,     0,   335,     0,
     335,   363,   335,   472,   335,     0,     0,   472,   374,   381,
     472,   386,   329,   335,    -5,     1,   472,     0,     0,     0,
     397,     0,   398,     0,     0,     2,     3,     0,     0,   399,
       0,     0,     4,     5,     6,     7,     0,     0,     8,     9,
      48,   476,     0,     0,     0,     0,     0,    11,     0,     0,
     400,   401,   402,   403,   404,   411,     0,   412,     0,   405,
     406,   407,   408,     0,   414,   338,     0,   409,   305,   410,
     339,   305,     0,     0,   447,     0,     0,     0,   548,   335,
       0,    49,     0,     0,     0,   415,   416,   417,   418,   419,
       0,     0,     0,     0,   420,   421,   422,   423,     0,   468,
       0,   215,   900,     0,   425,   468,     0,     0,   472,     0,
     411,     0,   412,   472,     0,   472,     0,     0,     0,   414,
       0,   329,     0,     0,   543,     0,     0,     0,     0,     0,
       0,   305,   363,   226,   237,   238,     0,   239,    26,    34,
     415,   416,   417,   418,   419,     0,   545,     0,     0,   420,
     421,   422,   423,   565,   472,   363,   544,   424,   571,   425,
     363,   579,   582,   584,   586,   589,   592,   595,     0,     0,
     598,     0,   602,     0,     0,   614,   619,   622,   624,   626,
     629,   632,   635,     0,   411,   638,   412,   413,     0,     0,
       0,   249,   468,   414,   651,   651,   468,     0,     0,   658,
     661,   664,   667,   670,   673,     0,     0,   253,   254,     0,
       0,     0,     0,     0,   415,   416,   417,   418,   419,   363,
     685,     0,     0,   420,   421,   422,   423,     0,   226,   237,
     238,   424,     0,   425,   732,   734,   735,   736,   737,   738,
     739,   740,   741,     0,   257,   258,   259,   260,   261,   262,
     263,   264,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,     0,     0,     0,     0,   468,     0,     0,
       0,     0,     0,     0,   363,   363,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     722,     0,   468,   329,   329,     0,     0,     0,     0,     0,
     731,   731,   731,   731,   731,   731,   731,   731,   731,   614,
     386,   386,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   756,   329,   257,
     258,   259,   260,   261,   262,   263,   264,   265,   266,   267,
     268,   269,   270,   271,   272,   273,   274,   275,   276,   277,
     278,   279,   280,   281,   282,   283,   284,   285,     0,     0,
       0,     0,     0,   374,   397,     0,   398,     0,   756,     0,
       0,     0,     0,   399,     0,     0,   292,     0,   542,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   400,   401,   402,   403,   404,   374,
       0,     0,     0,   405,   406,   407,   408,     0,     0,   338,
     461,   409,     0,   410,   339,     0,     0,     0,     0,     0,
       0,     0,   548,     0,   397,     0,   398,     0,   785,     0,
       0,     0,     0,   399,     0,     0,     0,  -152,  -145,     0,
       0,     0,   363,   468,     0,   651,   651,   468,     0,     0,
       0,     0,     0,     0,   400,   401,   402,   403,   404,     0,
       0,     0,   363,   405,   406,   407,   408,     0,  -145,   338,
       0,   409,     0,   410,   339,     0,     0,     0,  -152,     0,
       0,   329,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   386,     0,     0,     0,   822,
       0,   824,   331,   363,   468,   363,     0,     0,   468,     0,
       0,   468,   651,     0,   363,     0,     0,   468,     0,     0,
     348,     0,     0,   651,     0,     0,     0,     0,     0,     0,
       0,   651,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   368,     0,     0,   378,   382,     0,   387,   331,
       0,   411,     0,   412,     0,     0,     0,     0,     0,   651,
     414,     0,   873,     0,  -151,  -144,     0,     0,     0,     0,
       0,   386,     0,     0,   227,     0,   232,     0,     0,     0,
     886,   415,   416,   417,   418,   419,     0,     0,     0,     0,
     420,   421,   422,   423,     0,  -144,     0,     0,   424,     0,
     425,   397,     0,   398,     0,  -151,     0,     0,     0,   468,
     399,   448,     0,     0,   468,   542,   468,     0,     0,     0,
       0,   374,     0,     0,     0,     0,     0,     0,   873,     0,
       0,   400,   401,   402,   403,   404,     0,     0,     0,     0,
     405,   406,   407,   408,     0,   651,   338,   461,   409,     0,
     410,   339,    26,     0,   651,   468,     0,     0,   331,     0,
       0,   651,   873,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   651,   651,   546,     0,     0,     0,   292,     0,     0,
     651,     0,     0,     0,   292,   572,     0,     0,     0,   583,
     585,   587,   590,   593,   596,     0,     0,   599,     0,   603,
       0,     0,   615,     0,   623,   625,   627,   630,   633,   636,
       0,     0,   639,     0,     0,     0,     0,     0,     0,     0,
       0,   652,   652,     0,     0,     0,   659,   662,   665,   668,
     671,   674,     0,   364,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   228,   229,   686,   257,   258,
     259,   260,   261,   262,   263,   264,   265,   266,   267,   268,
     269,   270,   271,   272,   273,   274,   275,   276,   277,   278,
     279,   280,   281,   282,   283,   284,   285,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   292,     0,     0,     0,     0,     0,   456,
       0,     0,   717,   717,     0,     0,     0,   724,     0,   463,
     331,   331,     0,   438,     0,   478,     0,   387,   387,   387,
     387,   387,   387,   387,   387,   387,   292,   387,   387,     0,
     232,   520,   292,     0,     0,     0,     0,  -355,  -355,  -355,
    -355,  -355,  -355,   456,   533,   331,   439,   440,  -355,  -355,
       0,     0,   441,   442,     0,     0,     0,     0,     0,     0,
     443,  -355,   444,     0,     0,     0,     0,   533,     0,     0,
       0,     0,   364,   347,     0,     0,     0,     0,     0,   397,
     378,   398,     0,   601,     0,   607,     0,   616,   399,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   641,     0,
       0,     0,   232,   232,     0,   646,     0,     0,     0,   400,
     401,   402,   403,   404,     0,     0,   378,   676,   405,   406,
     407,   408,     0,     0,   338,     0,   409,     0,   410,   339,
     684,   533,     0,     0,     0,   689,     0,   690,     0,     0,
       0,   397,     0,   398,     0,     0,     0,     0,     0,     0,
     399,     0,     0,     0,     0,     0,     0,     0,   347,     0,
       0,     0,   652,   652,     0,     0,     0,     0,     0,   799,
       0,   400,   401,   402,   403,   404,     0,     0,     0,   228,
     405,   406,   407,   408,     0,   347,   338,     0,   717,   228,
     410,   339,     0,     0,     0,     0,   364,   364,   331,     0,
       0,     0,   347,     0,     0,     0,     0,   719,   719,     0,
     229,     0,   387,   292,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   228,   531,     0,     0,     0,   411,   652,
     412,   616,     0,     0,     0,     0,     0,   414,   764,     0,
     652,     0,     0,     0,     0,     0,     0,   531,   652,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   415,   416,
     417,   418,   419,   397,     0,   398,   611,   420,   421,   422,
     423,     0,   399,     0,   717,   424,   652,   425,   717,   874,
       0,     0,   229,   229,     0,     0,   292,     0,   387,   292,
       0,     0,     0,   400,   401,   402,   403,   404,     0,     0,
       0,   333,   405,   406,   407,   408,     0,     0,   338,     0,
     409,   531,   410,   339,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   347,   411,     0,   412,     0,
       0,   347,     0,     0,     0,   414,     0,     0,   378,     0,
       0,     0,     0,     0,   380,   874,     0,   390,   333,   292,
       0,     0,     0,     0,     0,     0,   415,   416,   417,   418,
     419,     0,   652,     0,   533,   420,   421,   422,   423,     0,
     796,   652,   411,   424,   412,   425,     0,     0,   652,   874,
       0,   414,     0,     0,   364,     0,     0,   718,   718,     0,
       0,     0,     0,   719,     0,     0,     0,     0,   652,   652,
       0,     0,   415,   416,   417,   418,   419,   652,     0,     0,
       0,   420,   421,   422,   423,     0,     0,     0,     0,   424,
       0,   425,     0,     0,     0,   364,     0,   364,     0,     0,
       0,     0,     0,     0,     0,   474,   364,     0,     0,     0,
     480,   474,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   333,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   719,
       0,     0,   390,   719,   875,     0,     0,     0,     0,   567,
       0,     0,     0,     0,   573,     0,     0,     0,     0,   347,
     588,   591,   594,   597,     0,     0,   600,     0,   604,   347,
     347,     0,     0,     0,     0,   628,   631,   634,   637,     0,
       0,   640,     0,     0,     0,     0,   901,     0,   474,     0,
     653,   653,   474,     0,     0,   660,   663,   666,   669,   672,
     675,     0,     0,     0,   531,     0,     0,     0,     0,     0,
     875,     0,   347,     0,     0,     0,   687,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   718,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   875,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   232,   934,     0,     0,     0,     0,
       0,     0,     0,   226,   237,   238,     0,   239,    26,    34,
     201,     0,     0,   474,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   347,     0,   347,   203,   725,     0,   474,   333,
     333,     0,     0,     0,     0,     0,   733,   733,   733,   733,
     733,   733,   733,   733,   733,     0,   390,   390,   205,   718,
       0,   249,     0,   718,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   333,     0,     0,   253,   254,     0,
       0,     0,     0,     0,     0,   214,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   380,
       0,     0,     0,     0,   257,   258,   259,   260,   261,   262,
     263,   264,   265,   266,   267,   268,   269,   270,   271,   272,
     273,   274,   275,   276,   277,   278,   279,   280,   281,   282,
     283,   284,   285,     0,     0,   380,     0,     0,     0,     0,
       0,     0,     0,     0,   347,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   229,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   474,
       0,   653,   653,   474,     0,     0,     0,     0,   800,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     713,     0,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,   333,     0,     0,
       0,   240,     0,     0,     0,   -91,     0,     0,     0,     0,
       0,   390,     0,     0,   328,   823,     0,   825,     0,     0,
     474,     0,     0,     0,   474,     0,     0,   474,   653,     0,
       0,     0,     0,   474,     0,   247,   -91,   248,     0,   653,
     249,     0,     0,     0,     0,     0,   -91,   653,     0,   207,
     208,   209,   210,   211,   212,   213,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   653,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   390,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,     0,     0,     0,   474,     0,     0,     0,     0,
     474,     0,   474,     0,     0,     0,     0,   380,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     236,   653,   226,   237,   238,     0,   239,    26,    34,   201,
     653,   474,     0,     0,     0,     0,     0,   653,     0,     0,
       0,   558,     0,     0,     0,    10,     0,   241,   242,   243,
     244,   245,     0,     0,   246,     0,     0,   653,   653,     0,
       0,     0,     0,     0,     0,     0,   653,     0,     0,   559,
     560,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,   250,    12,   251,   252,   207,
     208,   209,   210,   211,   212,   213,   253,   254,     0,     0,
       0,     0,     0,   255,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   236,     0,   226,   237,   238,     0,   239,    26,
      34,   201,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   240,   -60,     0,     0,   -60,     0,   241,
     242,   243,   244,   245,     0,     0,   246,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   247,     0,   248,
       0,     0,   249,     0,     0,     0,     0,   250,   -60,   251,
     252,   207,   208,   209,   210,   211,   212,   213,   253,   254,
       0,     0,     0,     0,     0,   255,   256,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   257,   258,   259,   260,   261,
     262,   263,   264,   265,   266,   267,   268,   269,   270,   271,
     272,   273,   274,   275,   276,   277,   278,   279,   280,   281,
     282,   283,   284,   285,   236,     0,   226,   237,   238,     0,
     239,    26,    34,   201,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   240,   -60,     0,     0,   -60,
       0,   241,   242,   243,   244,   245,     0,     0,   246,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   612,
       0,   248,     0,     0,   249,     0,     0,     0,     0,   250,
     -60,   251,   252,   207,   208,   209,   210,   211,   212,   213,
     253,   254,     0,     0,     0,     0,     0,   255,   256,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   257,   258,   259,
     260,   261,   262,   263,   264,   265,   266,   267,   268,   269,
     270,   271,   272,   273,   274,   275,   276,   277,   278,   279,
     280,   281,   282,   283,   284,   285,   236,     0,   226,   237,
     238,     0,   239,    26,    34,   201,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   240,     0,     0,
       0,    48,     0,   241,   242,   243,   244,   245,     0,     0,
     246,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   247,     0,   248,     0,     0,   249,     0,     0,     0,
       0,   250,    49,   251,   252,   207,   208,   209,   210,   211,
     212,   213,   253,   254,     0,     0,     0,     0,     0,   255,
     256,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   257,
     258,   259,   260,   261,   262,   263,   264,   265,   266,   267,
     268,   269,   270,   271,   272,   273,   274,   275,   276,   277,
     278,   279,   280,   281,   282,   283,   284,   285,   236,     0,
     226,   237,   238,     0,   239,    26,    34,   201,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   558,
       0,     0,     0,    10,     0,   241,   242,   243,   244,   245,
       0,     0,   246,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   247,     0,   248,     0,     0,   249,     0,
       0,     0,     0,   250,    12,   251,   252,   207,   208,   209,
     210,   211,   212,   213,   253,   254,     0,     0,     0,     0,
       0,   255,   256,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   257,   258,   259,   260,   261,   262,   263,   264,   265,
     266,   267,   268,   269,   270,   271,   272,   273,   274,   275,
     276,   277,   278,   279,   280,   281,   282,   283,   284,   285,
     713,     0,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,    48,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,    49,     0,     0,   207,
     208,   209,   210,   211,   212,   213,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,    97,    98,     0,     0,     0,    99,     0,     0,
     100,   101,     0,   102,     0,     0,   103,   104,   105,   106,
     107,     0,     0,   108,   109,     0,     0,   110,   111,   112,
     113,   114,   115,     0,   116,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   117,   118,   119,     0,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
     143,   144,   145,   146,   147,   148,   149,   150,   151,   152,
     153,   154,   155,   156,   157,   158,   159,   160,   161,   162,
     163,   164,   165,   166,   167,   168,   169,   170,   171,   172,
     173,   174,   175,   176,   177,   178,   179,   180,   181,   182,
     183,   184,   185,   186,   187,   188,   189,   190,   191,   192,
     193,   194,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   647,   648,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,   833,
     834,   835,   836,   837,   838,   839,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,    48,     0,   465,     0,     0,
       0,     0,     0,     0,   466,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,    49,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,   467,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,   465,     0,     0,
       0,     0,     0,     0,   466,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,   467,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   647,   648,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   870,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,   467,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,   467,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   385,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   328,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   612,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   226,   237,   238,     0,   239,    26,    34,   201,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   240,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   776,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   247,     0,   248,     0,     0,
     249,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   253,   254,     0,     0,
       0,     0,     0,     0,   256,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   257,   258,   259,   260,   261,   262,   263,
     264,   265,   266,   267,   268,   269,   270,   271,   272,   273,
     274,   275,   276,   277,   278,   279,   280,   281,   282,   283,
     284,   285,   482,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   483,   484,   485,   486,     0,     0,
     487,   488,   489,   490,   491,   492,   493,   494,   495,   496,
     497,   498,   499,   500,   501,   502,   503,   504,   505,   506,
     507,   508,   509,   510,   511,   512,   513,   514
};
static const yytype_int16 yycheck[] =
{
       0,     3,     4,     5,     6,     7,     8,   371,    69,    11,
     305,    78,    69,   482,    14,   648,    43,   426,    45,   434,
     535,   204,    24,   692,   385,    69,    77,   696,    30,    40,
      41,   249,   745,    84,   646,    67,   240,   792,   793,   688,
     811,    58,    44,    16,    18,    13,    63,    16,    75,    76,
      50,   694,   695,    55,    27,    57,     0,    59,    27,   814,
      62,    63,    64,    65,    12,   429,   430,    69,   251,    80,
      72,    73,     3,     9,   344,    77,    49,     8,     9,    10,
     366,   367,    84,    39,    52,    53,    88,     5,     6,     7,
       8,     8,     9,    11,    62,     8,     9,    10,    15,   870,
       1,     8,     9,    10,    35,   818,   877,     8,   821,    22,
       8,     9,    23,    13,    15,    26,   871,     8,     9,    23,
      56,     8,    35,    27,    34,    56,    44,    58,    35,    14,
      28,    27,     8,     9,    10,    26,    49,   780,   858,    58,
     911,    58,   791,    56,    35,    58,    22,    56,    58,    56,
       1,    58,    53,    15,    85,   926,    67,     8,   913,    35,
     446,   431,   448,    59,    15,   435,    15,   199,   881,   781,
      68,    26,    85,    49,     8,     9,    67,   360,    85,    47,
      56,    15,    58,   478,   904,    26,     8,     9,    10,   858,
     394,    53,    27,    22,   863,   399,   865,   224,   559,   560,
      34,    26,    53,   205,     4,     5,     6,     7,     8,    85,
     414,    11,    67,    35,    16,    83,    57,    16,   220,   852,
      23,   223,    57,    26,    58,    27,    67,    22,    27,    63,
       8,     9,   450,   702,   703,   904,    58,    15,   240,   241,
     242,   243,    67,   245,    44,   247,   516,    49,    16,    27,
      27,   894,    13,   255,    26,    55,   313,    57,   214,    27,
     903,     8,     9,    85,    67,    16,    26,    27,    15,    69,
      27,   541,   228,   229,    27,    26,    27,    77,    78,   340,
      58,    59,    59,   340,    84,    57,   286,    34,    88,    36,
      27,   806,    15,    27,    27,    67,   340,    57,    49,    22,
     256,   352,    59,   305,    57,   223,    57,    67,    26,    52,
      53,    58,    26,    27,    27,   317,    67,    53,    27,    62,
     524,    15,    59,   325,   326,    59,    59,   691,    22,   396,
     332,    54,    55,    26,   336,    58,    14,   339,   340,    57,
      63,   343,   369,   400,    57,    59,   761,   349,    57,    67,
     352,   646,    27,    67,   558,   357,   400,   772,   415,   542,
     543,   656,   657,    27,    57,     8,     9,    27,   370,   371,
      13,   415,    15,   400,    67,    27,   376,    13,    15,    15,
       8,     9,   393,   744,    59,    22,    22,    15,   415,    27,
     392,    34,   394,   808,    13,    59,   398,   399,   400,    59,
     356,    27,     8,     9,   699,   205,    34,    43,    36,    27,
     412,   413,    27,   415,    27,    58,    52,    53,    54,    55,
      27,    59,   222,   223,   451,    27,    62,   429,   430,   612,
      58,    26,    27,    59,    26,    27,    27,     8,     9,    57,
     240,   241,   242,   243,    59,   245,    59,    27,   450,    27,
      57,     3,   861,   862,    15,    58,   458,    59,   525,   820,
      63,    22,    23,    34,    59,    13,    27,    59,    59,   878,
     472,    49,    67,   534,   535,    67,   478,    57,    26,    27,
      23,    23,    16,   753,    27,    27,   781,   757,    22,    23,
     760,   558,    26,    27,    17,   910,   766,   564,    46,    47,
     527,    50,    51,   912,    52,    53,    49,   711,    17,    57,
      59,    15,    60,    27,    62,   930,   931,   317,    22,    67,
      13,   523,   524,   932,   939,   325,    34,    13,    23,    27,
      64,    65,    27,    67,    63,    49,    27,   539,   338,   339,
     340,   568,    15,   343,   344,     1,    13,   730,   752,    22,
     754,    49,   352,    13,    49,    11,    12,   921,    49,   763,
      46,    47,    18,    19,    20,    21,    52,    53,    24,    25,
      26,   371,    34,    58,   576,   531,    62,    33,    27,    46,
      47,    54,    55,    27,    22,    52,    53,    22,    15,   389,
      52,    53,   392,    60,   394,    62,   396,    15,    15,   399,
      49,    52,    53,    13,    22,    49,   901,    34,    59,    36,
     612,    67,   412,    23,    16,   617,    34,    27,    36,    58,
     677,    13,    15,    15,   580,   581,     0,     1,     3,    22,
      22,   431,    58,     8,     9,   435,    59,    63,    15,    13,
      14,    15,    16,    17,   646,    22,   648,     5,     6,    15,
      58,    43,    26,    23,   656,   657,    22,    27,   458,    15,
      52,    53,    54,    55,   620,   621,    58,     8,     9,    10,
      62,    63,    46,    47,    22,   677,    15,    15,    52,    53,
       0,     1,    15,    22,    22,    16,   688,    28,    56,   691,
      27,    11,    12,    67,    35,    64,    65,   699,    18,    19,
      20,    21,    64,    65,    24,    25,    26,    23,   708,   711,
      56,    27,    27,    33,   714,    56,   516,    58,    59,    27,
      63,   721,    23,   523,   524,   525,    27,    15,    58,    70,
      71,    72,    73,    74,    75,    76,    23,    58,    23,    23,
      27,   541,    27,    27,    85,   806,   748,    67,   750,   810,
     752,    23,   754,    23,     8,    27,   712,    27,   558,   761,
      34,   763,    15,    56,   564,    46,     8,     9,    10,    22,
      46,    23,    55,    59,    47,    27,   576,   779,   778,   781,
      69,    58,    27,    23,    63,   787,    28,    27,    34,   791,
      58,    58,    56,    35,   794,    34,    34,    34,    58,    52,
      53,    54,    55,   830,    34,    58,    50,    34,    59,    62,
      63,    58,    22,    34,    56,   771,    58,   617,    79,    34,
     847,    24,    23,    30,   525,   315,   810,   829,    70,    71,
      72,    73,    74,    75,    76,    37,    38,    39,    40,    41,
      42,   720,   877,    85,   617,   911,    48,    49,   648,   677,
     852,   523,   539,   891,   323,   321,   340,   357,   814,    61,
       0,     1,   249,    -1,    -1,    -1,   866,    15,   870,    -1,
      -1,    11,    12,    -1,    22,    -1,    -1,   677,    18,    19,
      20,    21,    -1,    -1,    24,    25,    26,    -1,   688,   891,
      -1,   691,   692,    33,    -1,    -1,   696,    -1,    -1,   901,
      -1,    -1,   902,   859,    52,    53,    54,    55,    -1,    -1,
      -1,   711,    -1,    -1,    62,   204,   205,    -1,    -1,   921,
      -1,    -1,    -1,    -1,    -1,   925,    -1,    67,   884,    -1,
      -1,    -1,    -1,   889,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   897,    -1,    -1,    -1,   745,    -1,    -1,   748,    -1,
     750,   240,   752,   753,   754,    -1,    -1,   757,   247,   248,
     760,   250,   251,   763,     0,     1,   766,    -1,    -1,    -1,
      13,    -1,    15,    -1,    -1,    11,    12,    -1,    -1,    22,
      -1,    -1,    18,    19,    20,    21,    -1,    -1,    24,    25,
      26,   791,    -1,    -1,    -1,    -1,    -1,    33,    -1,    -1,
      43,    44,    45,    46,    47,    13,    -1,    15,    -1,    52,
      53,    54,    55,    -1,    22,    58,    -1,    60,   818,    62,
      63,   821,    -1,    -1,   313,    -1,    -1,    -1,    71,   829,
      -1,    67,    -1,    -1,    -1,    43,    44,    45,    46,    47,
      -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,   338,
      -1,   340,   852,    -1,    62,   344,    -1,    -1,   858,    -1,
      13,    -1,    15,   863,    -1,   865,    -1,    -1,    -1,    22,
      -1,   360,    -1,    -1,    27,    -1,    -1,    -1,    -1,    -1,
      -1,   881,   371,     3,     4,     5,    -1,     7,     8,     9,
      43,    44,    45,    46,    47,    -1,   385,    -1,    -1,    52,
      53,    54,    55,   392,   904,   394,    59,    60,   397,    62,
     399,   400,   401,   402,   403,   404,   405,   406,    -1,    -1,
     409,    -1,   411,    -1,    -1,   414,   415,   416,   417,   418,
     419,   420,   421,    -1,    13,   424,    15,    16,    -1,    -1,
      -1,    61,   431,    22,   433,   434,   435,    -1,    -1,   438,
     439,   440,   441,   442,   443,    -1,    -1,    77,    78,    -1,
      -1,    -1,    -1,    -1,    43,    44,    45,    46,    47,   458,
     459,    -1,    -1,    52,    53,    54,    55,    -1,     3,     4,
       5,    60,    -1,    62,   549,   550,   551,   552,   553,   554,
     555,   556,   557,    -1,   114,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,    -1,    -1,    -1,    -1,   516,    -1,    -1,
      -1,    -1,    -1,    -1,   523,   524,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     539,    -1,   541,   542,   543,    -1,    -1,    -1,    -1,    -1,
     549,   550,   551,   552,   553,   554,   555,   556,   557,   558,
     559,   560,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   576,   577,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,    -1,    -1,
      -1,    -1,    -1,   612,    13,    -1,    15,    -1,   617,    -1,
      -1,    -1,    -1,    22,    -1,    -1,    78,    -1,    27,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    43,    44,    45,    46,    47,   648,
      -1,    -1,    -1,    52,    53,    54,    55,    -1,    -1,    58,
      59,    60,    -1,    62,    63,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    71,    -1,    13,    -1,    15,    -1,   677,    -1,
      -1,    -1,    -1,    22,    -1,    -1,    -1,    26,    27,    -1,
      -1,    -1,   691,   692,    -1,   694,   695,   696,    -1,    -1,
      -1,    -1,    -1,    -1,    43,    44,    45,    46,    47,    -1,
      -1,    -1,   711,    52,    53,    54,    55,    -1,    57,    58,
      -1,    60,    -1,    62,    63,    -1,    -1,    -1,    67,    -1,
      -1,   730,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   744,    -1,    -1,    -1,   748,
      -1,   750,   204,   752,   753,   754,    -1,    -1,   757,    -1,
      -1,   760,   761,    -1,   763,    -1,    -1,   766,    -1,    -1,
     222,    -1,    -1,   772,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   780,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   244,    -1,    -1,   247,   248,    -1,   250,   251,
      -1,    13,    -1,    15,    -1,    -1,    -1,    -1,    -1,   808,
      22,    -1,   811,    -1,    26,    27,    -1,    -1,    -1,    -1,
      -1,   820,    -1,    -1,    71,    -1,    73,    -1,    -1,    -1,
     829,    43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,    -1,    57,    -1,    -1,    60,    -1,
      62,    13,    -1,    15,    -1,    67,    -1,    -1,    -1,   858,
      22,   313,    -1,    -1,   863,    27,   865,    -1,    -1,    -1,
      -1,   870,    -1,    -1,    -1,    -1,    -1,    -1,   877,    -1,
      -1,    43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,    -1,   894,    58,    59,    60,    -1,
      62,    63,     8,    -1,   903,   904,    -1,    -1,   360,    -1,
      -1,   910,   911,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   930,   931,   385,    -1,    -1,    -1,   389,    -1,    -1,
     939,    -1,    -1,    -1,   396,   397,    -1,    -1,    -1,   401,
     402,   403,   404,   405,   406,    -1,    -1,   409,    -1,   411,
      -1,    -1,   414,    -1,   416,   417,   418,   419,   420,   421,
      -1,    -1,   424,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   433,   434,    -1,    -1,    -1,   438,   439,   440,   441,
     442,   443,    -1,   240,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    72,    73,   459,   114,   115,
     116,   117,   118,   119,   120,   121,   122,   123,   124,   125,
     126,   127,   128,   129,   130,   131,   132,   133,   134,   135,
     136,   137,   138,   139,   140,   141,   142,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   525,    -1,    -1,    -1,    -1,    -1,   326,
      -1,    -1,   534,   535,    -1,    -1,    -1,   539,    -1,   336,
     542,   543,    -1,    13,    -1,   342,    -1,   549,   550,   551,
     552,   553,   554,   555,   556,   557,   558,   559,   560,    -1,
     357,   358,   564,    -1,    -1,    -1,    -1,    37,    38,    39,
      40,    41,    42,   370,   371,   577,    46,    47,    48,    49,
      -1,    -1,    52,    53,    -1,    -1,    -1,    -1,    -1,    -1,
      60,    61,    62,    -1,    -1,    -1,    -1,   394,    -1,    -1,
      -1,    -1,   399,   220,    -1,    -1,    -1,    -1,    -1,    13,
     612,    15,    -1,   410,    -1,   412,    -1,   414,    22,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   425,    -1,
      -1,    -1,   429,   430,    -1,   432,    -1,    -1,    -1,    43,
      44,    45,    46,    47,    -1,    -1,   648,   444,    52,    53,
      54,    55,    -1,    -1,    58,    -1,    60,    -1,    62,    63,
     457,   458,    -1,    -1,    -1,   462,    -1,   464,    -1,    -1,
      -1,    13,    -1,    15,    -1,    -1,    -1,    -1,    -1,    -1,
      22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   305,    -1,
      -1,    -1,   694,   695,    -1,    -1,    -1,    -1,    -1,   701,
      -1,    43,    44,    45,    46,    47,    -1,    -1,    -1,   326,
      52,    53,    54,    55,    -1,   332,    58,    -1,   720,   336,
      62,    63,    -1,    -1,    -1,    -1,   523,   524,   730,    -1,
      -1,    -1,   349,    -1,    -1,    -1,    -1,   534,   535,    -1,
     357,    -1,   744,   745,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   370,   371,    -1,    -1,    -1,    13,   761,
      15,   558,    -1,    -1,    -1,    -1,    -1,    22,    23,    -1,
     772,    -1,    -1,    -1,    -1,    -1,    -1,   394,   780,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    43,    44,
      45,    46,    47,    13,    -1,    15,   413,    52,    53,    54,
      55,    -1,    22,    -1,   806,    60,   808,    62,   810,   811,
      -1,    -1,   429,   430,    -1,    -1,   818,    -1,   820,   821,
      -1,    -1,    -1,    43,    44,    45,    46,    47,    -1,    -1,
      -1,   204,    52,    53,    54,    55,    -1,    -1,    58,    -1,
      60,   458,    62,    63,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   472,    13,    -1,    15,    -1,
      -1,   478,    -1,    -1,    -1,    22,    -1,    -1,   870,    -1,
      -1,    -1,    -1,    -1,   247,   877,    -1,   250,   251,   881,
      -1,    -1,    -1,    -1,    -1,    -1,    43,    44,    45,    46,
      47,    -1,   894,    -1,   691,    52,    53,    54,    55,    -1,
     697,   903,    13,    60,    15,    62,    -1,    -1,   910,   911,
      -1,    22,    -1,    -1,   711,    -1,    -1,   534,   535,    -1,
      -1,    -1,    -1,   720,    -1,    -1,    -1,    -1,   930,   931,
      -1,    -1,    43,    44,    45,    46,    47,   939,    -1,    -1,
      -1,    52,    53,    54,    55,    -1,    -1,    -1,    -1,    60,
      -1,    62,    -1,    -1,    -1,   752,    -1,   754,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   338,   763,    -1,    -1,    -1,
     343,   344,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   360,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   806,
      -1,    -1,   385,   810,   811,    -1,    -1,    -1,    -1,   392,
      -1,    -1,    -1,    -1,   397,    -1,    -1,    -1,    -1,   646,
     403,   404,   405,   406,    -1,    -1,   409,    -1,   411,   656,
     657,    -1,    -1,    -1,    -1,   418,   419,   420,   421,    -1,
      -1,   424,    -1,    -1,    -1,    -1,   853,    -1,   431,    -1,
     433,   434,   435,    -1,    -1,   438,   439,   440,   441,   442,
     443,    -1,    -1,    -1,   691,    -1,    -1,    -1,    -1,    -1,
     877,    -1,   699,    -1,    -1,    -1,   459,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   720,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   911,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   921,   922,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,     3,     4,     5,    -1,     7,     8,     9,
      10,    -1,    -1,   516,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   779,    -1,   781,    35,   539,    -1,   541,   542,
     543,    -1,    -1,    -1,    -1,    -1,   549,   550,   551,   552,
     553,   554,   555,   556,   557,    -1,   559,   560,    58,   806,
      -1,    61,    -1,   810,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   577,    -1,    -1,    77,    78,    -1,
      -1,    -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   612,
      -1,    -1,    -1,    -1,   114,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,   142,    -1,    -1,   648,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   901,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   921,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   692,
      -1,   694,   695,   696,    -1,    -1,    -1,    -1,   701,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
       1,    -1,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   730,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    26,    -1,    -1,    -1,    -1,
      -1,   744,    -1,    -1,    35,   748,    -1,   750,    -1,    -1,
     753,    -1,    -1,    -1,   757,    -1,    -1,   760,   761,    -1,
      -1,    -1,    -1,   766,    -1,    56,    57,    58,    -1,   772,
      61,    -1,    -1,    -1,    -1,    -1,    67,   780,    -1,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   808,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   820,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,    -1,    -1,    -1,   858,    -1,    -1,    -1,    -1,
     863,    -1,   865,    -1,    -1,    -1,    -1,   870,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
       1,   894,     3,     4,     5,    -1,     7,     8,     9,    10,
     903,   904,    -1,    -1,    -1,    -1,    -1,   910,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    26,    -1,    28,    29,    30,
      31,    32,    -1,    -1,    35,    -1,    -1,   930,   931,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   939,    -1,    -1,    50,
      51,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    66,    67,    68,    69,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    -1,    -1,
      -1,    -1,    -1,    84,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     1,    -1,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    22,    23,    -1,    -1,    26,    -1,    28,
      29,    30,    31,    32,    -1,    -1,    35,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,    -1,    -1,    -1,    -1,    66,    67,    68,
      69,    70,    71,    72,    73,    74,    75,    76,    77,    78,
      -1,    -1,    -1,    -1,    -1,    84,    85,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   114,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,   142,     1,    -1,     3,     4,     5,    -1,
       7,     8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    23,    -1,    -1,    26,
      -1,    28,    29,    30,    31,    32,    -1,    -1,    35,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,
      -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    66,
      67,    68,    69,    70,    71,    72,    73,    74,    75,    76,
      77,    78,    -1,    -1,    -1,    -1,    -1,    84,    85,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,     1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,
      -1,    26,    -1,    28,    29,    30,    31,    32,    -1,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,
      -1,    66,    67,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    77,    78,    -1,    -1,    -1,    -1,    -1,    84,
      85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   142,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    22,
      -1,    -1,    -1,    26,    -1,    28,    29,    30,    31,    32,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    66,    67,    68,    69,    70,    71,    72,
      73,    74,    75,    76,    77,    78,    -1,    -1,    -1,    -1,
      -1,    84,    85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   114,   115,   116,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   142,
       1,    -1,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    26,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    67,    -1,    -1,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,    -1,    -1,    -1,     8,    -1,    -1,
      11,    12,    -1,    14,    -1,    -1,    17,    18,    19,    20,
      21,    -1,    -1,    24,    25,    -1,    -1,    28,    29,    30,
      31,    32,    33,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    64,    65,    66,    -1,    68,    69,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    79,    80,
      81,    82,    83,    84,    85,    86,    87,    88,    89,    90,
      91,    92,    93,    94,    95,    96,    97,    98,    99,   100,
     101,   102,   103,   104,   105,   106,   107,   108,   109,   110,
     111,   112,   113,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    55,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    70,
      71,    72,    73,    74,    75,    76,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    26,    -1,    28,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    67,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    79,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    28,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    79,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    55,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    79,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    79,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,     3,     4,     5,    -1,     7,     8,     9,    10,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    56,    -1,    58,    -1,    -1,
      61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,    -1,
      -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   114,   115,   116,   117,   118,   119,   120,
     121,   122,   123,   124,   125,   126,   127,   128,   129,   130,
     131,   132,   133,   134,   135,   136,   137,   138,   139,   140,
     141,   142,    58,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    80,    81,    82,    83,    -1,    -1,
      86,    87,    88,    89,    90,    91,    92,    93,    94,    95,
      96,    97,    98,    99,   100,   101,   102,   103,   104,   105,
     106,   107,   108,   109,   110,   111,   112,   113
};
/* YYSTOS[STATE-NUM] -- The symbol kind of the accessing symbol of
   state STATE-NUM.  */
static const yytype_uint8 yystos[] =
{
       0,     1,    11,    12,    18,    19,    20,    21,    24,    25,
      26,    33,    67,   149,   150,   151,   152,   158,   159,   160,
     161,   162,   163,   252,    12,     1,     8,    15,    53,   153,
     154,   155,   156,   225,     9,   157,   225,   226,   234,   235,
     157,   157,   157,   157,    18,   157,     0,   252,    26,    67,
     150,     1,   153,    15,    53,    13,   155,    14,     1,    15,
      16,    17,    46,    47,    52,    53,    56,   227,   228,    58,
     208,    15,    15,    22,   236,   208,   208,    22,    22,   167,
     157,   167,   151,   252,    13,    53,   233,   234,    14,   234,
     225,     5,     6,   225,   225,   225,   225,     3,     4,     8,
      11,    12,    14,    17,    18,    19,    20,    21,    24,    25,
      28,    29,    30,    31,    32,    33,    35,    64,    65,    66,
      68,    69,    70,    71,    72,    73,    74,    75,    76,    77,
      78,    79,    80,    81,    82,    83,    84,    85,    86,    87,
      88,    89,    90,    91,    92,    93,    94,    95,    96,    97,
      98,    99,   100,   101,   102,   103,   104,   105,   106,   107,
     108,   109,   110,   111,   112,   113,   114,   115,   116,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,   142,   229,   230,   231,   232,     9,
     228,    10,    28,    35,    56,    58,    59,    70,    71,    72,
      73,    74,    75,    76,    85,   187,   209,   210,   212,   216,
     234,   246,   249,    13,   164,   165,     3,   239,   226,   226,
     237,   238,   239,   167,   167,   233,     1,     4,     5,     7,
      22,    28,    29,    30,    31,    32,    35,    56,    58,    61,
      66,    68,    69,    77,    78,    84,    85,   114,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,   142,   168,   169,   170,   174,
     181,   187,   192,   193,   194,   195,   196,   197,   198,   207,
     211,   212,   216,   220,   225,   234,   239,   240,   241,   242,
     243,   244,   247,   249,   250,   208,   233,    27,   234,    17,
      17,    27,    57,    13,   228,    34,    15,   236,    35,   187,
     188,   192,   234,   247,   187,   234,    15,   236,    58,    63,
      27,    59,    15,    34,    58,    63,   214,   226,   192,   234,
     157,   167,    63,   166,   236,   236,    23,    27,    13,    23,
      56,   185,   186,   187,   239,   234,   234,   234,   192,   234,
      15,    22,    34,    36,   187,   188,   189,   191,   192,   225,
     247,   187,   192,   242,   250,    58,   187,   192,   223,   224,
     247,   188,    58,   225,    22,    23,   252,    13,    15,    22,
      43,    44,    45,    46,    47,    52,    53,    54,    55,    60,
      62,    13,    15,    16,    22,    43,    44,    45,    46,    47,
      52,    53,    54,    55,    60,    62,   200,   201,   202,    22,
      22,    58,    15,    34,    36,    58,   213,   214,    13,    46,
      47,    52,    53,    60,    62,   183,   184,   187,   192,   216,
      16,   164,   234,   230,   231,   234,   239,    15,    22,    27,
      57,    59,    15,   239,    15,    28,    35,    79,   187,   192,
     217,   219,   234,   245,   247,   215,   234,   210,   239,   234,
     247,   217,    58,    80,    81,    82,    83,    86,    87,    88,
      89,    90,    91,    92,    93,    94,    95,    96,    97,    98,
      99,   100,   101,   102,   103,   104,   105,   106,   107,   108,
     109,   110,   111,   112,   113,   251,    58,   233,   236,   238,
     239,   188,    23,    27,    49,    22,   201,   201,   167,   225,
     186,   226,   237,   239,    56,    56,    57,    27,    57,   190,
     252,    58,    27,    27,    59,   187,   192,   224,    71,    37,
      38,    39,    40,    41,    42,    48,    49,    61,    22,    50,
      51,   167,   169,   222,   252,   187,   199,   247,   208,   186,
     169,   187,   192,   247,   225,   186,    49,    56,   167,   187,
     212,   216,   187,   192,   187,   192,   187,   192,   247,   187,
     192,   247,   187,   192,   247,   187,   192,   247,   187,   192,
     247,   239,   187,   192,   247,   225,   234,   239,    28,    68,
     205,   226,    56,   186,   187,   192,   239,    49,   167,   187,
     212,   216,   187,   192,   187,   192,   187,   192,   247,   187,
     192,   247,   187,   192,   247,   187,   192,   247,   187,   192,
     247,   239,   202,   237,   237,   217,   239,    55,    56,   171,
     172,   187,   192,   247,   171,   217,    27,    27,   187,   192,
     247,   187,   192,   247,   187,   192,   247,   187,   192,   247,
     187,   192,   247,   187,   192,   247,   239,    27,   201,   201,
     225,   248,   250,   167,   239,   187,   192,   247,    63,   239,
     239,    22,    27,    59,    13,    34,    58,    15,   214,    59,
     251,    34,    52,    53,   217,    57,   185,   186,   168,   167,
      34,    49,    23,     1,   175,   176,   177,   192,   226,   239,
     249,   175,   187,   191,   192,   247,   217,   188,   188,    59,
      56,   187,   223,   247,   223,   223,   223,   223,   223,   223,
     223,   223,   224,   224,    64,    65,   221,   169,    46,    59,
      46,   167,    49,    58,    49,   182,   187,    58,   236,   236,
      58,    58,   188,    49,    23,   182,    58,   236,   236,    23,
      23,    59,    34,   213,   214,    55,    35,   170,   173,   234,
      27,    59,   214,   214,   183,   187,   216,    47,   215,   237,
     219,    63,   172,   172,   218,   219,   239,   214,    59,   192,
     247,   251,   251,    59,    23,    23,    56,   186,    34,    57,
     252,    34,   177,    57,    59,    59,    59,   188,   224,   222,
      64,    65,   187,   247,   187,   247,   186,   217,   186,    27,
      49,   217,   217,    70,    71,    72,    73,    74,    75,    76,
     171,   203,   204,   206,   225,    57,   186,    49,   217,   236,
     171,    57,   252,    15,   172,   213,   214,   225,    58,    23,
     215,   200,   200,    27,    59,   252,   175,    23,   171,   176,
      56,   178,   179,   187,   192,   239,   245,    34,   200,    57,
     222,   224,   222,    23,    59,    23,   187,   167,    59,    59,
      59,    27,    59,    59,    34,    23,   167,    59,    35,   170,
     234,   239,   218,    34,    58,   219,   219,    57,   180,   245,
      27,    49,   200,   178,   222,   236,   236,   204,   225,   172,
     236,    22,    15,    59,   172,   218,    27,    57,   171,   179,
      27,    27,   200,   237,   239,    59,   245,   171,   171,    27,
      23,   171
};
/* YYR1[RULE-NUM] -- Symbol kind of the left-hand side of rule RULE-NUM.  */
static const yytype_uint8 yyr1[] =
{
       0,   148,   149,   149,   149,   149,   149,   150,   150,   151,
     151,   151,   151,   151,   151,   151,   151,   152,   152,   152,
     152,   152,   152,   153,   153,   153,   153,   154,   154,   154,
     155,   155,   155,   155,   155,   155,   155,   156,   156,   156,
     156,   156,   156,   157,   157,   158,   158,   159,   160,   161,
     162,   163,   164,   165,   165,   166,   166,   167,   168,   168,
     168,   169,   169,   169,   169,   169,   169,   169,   169,   169,
     169,   169,   169,   170,   170,   170,   170,   171,   171,   172,
     172,   172,   172,   172,   173,   173,   174,   174,   174,   175,
     175,   175,   176,   176,   176,   176,   176,   176,   176,   176,
     176,   177,   177,   177,   178,   178,   179,   179,   179,   179,
     179,   180,   180,   181,   181,   181,   181,   181,   182,   182,
     183,   183,   184,   184,   185,   185,   185,   186,   186,   187,
     187,   187,   187,   187,   187,   187,   187,   187,   187,   187,
     187,   188,   188,   188,   188,   188,   188,   189,   189,   190,
     190,   191,   191,   191,   191,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   192,   192,   192,   192,   192,   192,   192,   192,
     192,   192,   193,   194,   195,   196,   197,   198,   199,   199,
     199,   199,   199,   199,   200,   200,   201,   201,   202,   202,
     202,   203,   203,   204,   205,   205,   205,   206,   206,   206,
     206,   206,   206,   206,   202,   207,   208,   208,   208,   209,
     209,   210,   210,   210,   210,   210,   210,   210,   210,   210,
     210,   210,   210,   210,   211,   211,   211,   212,   212,   212,
     213,   213,   214,   214,   215,   215,   216,   216,   216,   217,
     217,   217,   218,   218,   218,   218,   219,   219,   219,   219,
     219,   219,   219,   219,   219,   219,   220,   220,   220,   220,
     221,   221,   222,   222,   222,   223,   223,   223,   224,   224,
     224,   224,   224,   224,   224,   224,   224,   224,   224,   224,
     224,   224,   225,   226,   226,   227,   227,   227,   228,   229,
     229,   230,   230,   231,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   232,   232,   232,   232,   232,   232,   232,   232,
     232,   232,   233,   233,   233,   234,   235,   235,   236,   236,
     236,   236,   237,   237,   238,   238,   238,   238,   239,   240,
     241,   242,   242,   242,   243,   244,   244,   245,   246,   247,
     247,   247,   247,   247,   247,   247,   247,   248,   248,   249,
     249,   249,   249,   249,   249,   249,   250,   250,   250,   250,
     250,   250,   250,   250,   250,   250,   250,   250,   250,   250,
     250,   250,   250,   250,   250,   250,   250,   250,   250,   250,
     250,   250,   250,   250,   250,   251,   251,   251,   251,   251,
     251,   251,   251,   251,   251,   251,   251,   251,   251,   251,
     251,   251,   251,   251,   251,   251,   251,   251,   251,   251,
     251,   251,   251,   251,   251,   251,   251,   251,   251,   251,
     252,   252,   252,   252
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
       1,     1,     1,     3,     6,     3,     5,     3,     1,     1,
       1,     1,     3,     2,     3,     1,     5,     5,     7,     3,
       1,     0,     3,     5,     4,     6,     4,     5,     6,     7,
       1,     1,     1,     1,     3,     1,     1,     3,     1,     1,
       1,     3,     1,     3,     3,     1,     2,     2,     3,     1,
       1,     1,     3,     1,     1,     1,     3,     3,     1,     1,
       3,     1,     2,     3,     4,     1,     2,     3,     4,     1,
       3,     3,     3,     3,     1,     1,     1,     3,     1,     1,
       1,     1,     1,     1,     5,     1,     1,     2,     2,     2,
       2,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     4,     6,
       6,     6,     6,     3,     5,     4,     4,     3,     3,     4,
       6,     6,     6,     6,     3,     3,     3,     3,     5,     7,
       7,     7,     4,     4,     4,     4,     3,     6,     3,     6,
       5,     5,     5,     3,     4,     2,     3,     4,     1,     1,
       3,     3,     3,     3,     1,     2,     0,     1,     2,     5,
       5,     3,     1,     3,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     5,     4,     3,     2,     0,     3,
       1,     1,     3,     1,     3,     3,     3,     2,     2,     1,
       1,     3,     5,     5,     2,     4,     5,     2,     4,     5,
       3,     3,     1,     4,     1,     3,     6,     9,     8,     3,
       1,     0,     3,     3,     1,     0,     1,     1,     1,     1,
       1,     3,     4,     3,     4,     6,     3,     5,     4,     6,
       3,     4,     1,     2,     1,     1,     1,     1,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     5,
       3,     1,     1,     1,     2,     1,     2,     3,     3,     1,
       3,     1,     3,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     3,     2,     1,     1,     1,     2,     2,     3,
       3,     4,     1,     3,     1,     1,     3,     2,     1,     1,
       1,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     3,     3,     2,     2,     1,     3,     1,
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
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     2,     1,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
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
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    21,    23,     0,     0,     0,     0,     0,    25,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    27,     0,     0,     0,     0,     0,     0,    93,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     5,     0,     0,     0,     0,     7,
       0,     0,     0,     0,     0,     0,     0,    31,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    15,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      33,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    17,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     1,     0,     0,     0,     0,     0,     0,     3,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      67,     0,    69,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,    89,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    71,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      91,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    75,     0,     0,     0,     0,     0,
       0,    77,     0,     0,     0,     0,    79,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    45,
       0,     0,    53,     0,     0,     0,     0,     0,    55,    57,
       0,     0,    59,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    47,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      61,    63,     0,    65,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     9,     0,     0,    11,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    19,     0,     0,    81,     0,    13,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      29,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    43,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    73,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      83,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    35,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    37,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    39,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    41,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    51,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,    85,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    87,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    49,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0
};
/* YYCONFL[I] -- lists of conflicting rule numbers, each terminated by
   0, pointed into by YYCONFLP.  */
static const short yyconfl[] =
{
       0,   485,     0,   485,     0,   129,     0,   129,     0,   155,
       0,   156,     0,   373,     0,   129,     0,   129,     0,   512,
       0,   129,     0,   129,     0,   129,     0,   129,     0,   322,
       0,   129,     0,   129,     0,   152,     0,   152,     0,   151,
       0,   151,     0,   373,     0,   153,     0,   153,     0,   357,
       0,   371,     0,   156,     0,   156,     0,   156,     0,   156,
       0,   156,     0,   156,     0,   156,     0,   129,     0,   129,
       0,   129,     0,   130,     0,   486,     0,   486,     0,   494,
       0,   495,     0,   373,     0,   356,     0,   356,     0,   129,
       0,   129,     0,   373,     0
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
  "MCK_RETURN", "MCK_ERROR", "MCK_IO", "MCK_IN", "MCK_OUT", "MCK_NC",
  "MCK_PSRC", "MCK_PSNK", "MCK_PSBI", "MCONST_HIGH", "MCONST_LOW",
  "MCONST_NC", "MCU_INT", "MCU_HEX", "MCU_FLOAT", "MCU_STRING", "MCK_FUNC",
  "MCK_THIS", "MCU_VOLT", "MCU_AMP", "MCU_CAP", "MCU_IND", "MCU_TIME",
  "MCU_LEN", "MCU_WATT", "MCU_OHM", "MCU_TEMP", "MCU_HZ", "MCU_DB",
  "MCU_PPM", "MCU_PERCENT", "MCU_BAUD", "MCU_DATASIZE", "MCU_SPS",
  "MCU_SIEMENS", "MCU_RESPONSIVITY", "MCU_ANGLE", "MCU_ANGULAR_RATE",
  "MCU_ENERGY", "MCU_EFIELD", "MCU_HFIELD", "MCU_FLUX", "MCU_BFIELD",
  "MCU_SLEW", "MCU_NOISE", "MCU_CHARGE", "MCUVAL_VOLT", "MCUVAL_AMP",
  "MCUVAL_CAP", "MCUVAL_IND", "MCUVAL_TIME", "MCUVAL_LEN", "MCUVAL_WATT",
  "MCUVAL_OHM", "MCUVAL_TEMP", "MCUVAL_HZ", "MCUVAL_DB", "MCUVAL_PPM",
  "MCUVAL_PERCENT", "MCUVAL_BAUD", "MCUVAL_DATASIZE", "MCUVAL_SPS",
  "MCUVAL_SIEMENS", "MCUVAL_RESPONSIVITY", "MCUVAL_ANGLE",
  "MCUVAL_ANGULAR_RATE", "MCUVAL_ENERGY", "MCUVAL_EFIELD", "MCUVAL_HFIELD",
  "MCUVAL_FLUX", "MCUVAL_BFIELD", "MCUVAL_SLEW", "MCUVAL_NOISE",
  "MCUVAL_CHARGE", "MCUVAL_COMPOSITE", "MC_WS", "MC_SINGLE_COMMENT",
  "MC_MULTI_COMMENT", "UVALUE_WORD_PREC", "IDA_BASE_PREC", "$accept",
  "start", "mc_tops", "mc_top", "mc_use", "mc_uri", "mc_prefix",
  "mc_uri_trunk", "mc_levels", "mc_class_name", "mc_component",
  "mc_module", "mc_interface", "mc_enum", "mc_bom", "mc_recipe",
  "mc_component_derivs", "mc_component_variant", "mc_component_adopts",
  "mc_body", "mc_clauses", "mc_clause", "mc_attribute", "mc_attr_values",
  "mc_attr_value", "mc_attr_lines", "mc_attribute_pin", "mc_pins_lines",
  "mc_pins_line", "mc_pin_idn", "mc_pins_names", "mc_pins_name",
  "mc_nc_list", "mc_net", "mc_opds", "mc_net_port_elem",
  "mc_net_port_elems", "mc_mn_opd", "mc_mn_opds", "mc_opd", "mc_phrases",
  "mc_list_items", "mc_list_sep", "mc_list_item", "mc_phrase", "mc_role",
  "mc_conduit", "mc_domain", "mc_rail", "mc_partition", "mc_error_stmt",
  "mc_error_msg", "mc_tattrs", "mc_tattrs_opt", "mc_tattr",
  "mc_tattr_args", "mc_tattr_arg", "mc_tkey", "mc_resword", "mc_function",
  "mc_paramds", "mc_pards", "mc_pard", "mc_declare_a", "mc_declare_a1",
  "mc_insts", "mc_inst", "mc_iface_name", "mc_declare_b", "mc_params",
  "mc_body_params", "mc_param", "mc_conds", "mc_conds_elifs",
  "mc_cond_block", "mc_expr", "mc_judge", "mc_id", "mc_ida", "mc_ida_tail",
  "mc_ida_bracket", "mc_ida_items", "mc_ida_item", "mc_ida_run",
  "mc_ida_run_atom", "mc_idss", "mc_ids", "mc_idseg", "mc_idm", "mc_idans",
  "mc_idan", "mc_int", "mc_hex", "mc_float", "mc_number", "mc_string",
  "mc_const", "mc_nc", "mc_underscore", "mc_literal", "mc_cond_word",
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
                 { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1003_CLAUSE_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
    break;
  case 73:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 74:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link(
                            mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                            mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 75:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_ADD, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 76:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)->sub,
                            mc_value_create_node(MCAST_OPD_DOT,
                                mc_value_create_data(MCAST_IDA, strdup((char*)(YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->data), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->len))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 77:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 78:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 79:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
    break;
  case 80:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); 
}
    break;
  case 81:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); 
}
    break;
  case 82:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 83:
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
  case 84:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 85:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 86:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 87:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PINADD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 88:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 89:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 90:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 91:
                                                    { ((*yyvalp).value) = NULL; }
    break;
  case 92:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 93:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 94:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 95:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 96:
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
  case 97:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 98:
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
  case 99:
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
  case 100:
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1004_PIN_SKIPPED, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 101:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 102:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 103:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 104:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 105:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 106:
{
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 107:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME,
            mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 108:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 109:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 110:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 111:
                                         { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 112:
                                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 113:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 114:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 115:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 116:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 4 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 6 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 7 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 9)) {
        mc_dlog_add(MCD_E1007_NET_NOT_PORT, 1,
                    g_last_token->tpos, g_last_token->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 117:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link(
            mc_value_create_data(MCAST_IOTYPE_RETURN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 118:
                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 119:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 120:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 121:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 122:
                                                                 { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 123:
                                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 126:
          {
              
              ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
          }
    break;
  case 127:
                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 128:
                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 129:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 130:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 131:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, 
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 132:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 133:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 134:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 135:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD,
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 136:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 137:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen), 
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 138:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 139:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 140:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 141:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 142:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 143:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 144:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 145:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 146:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 147:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 148:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 149:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 150:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 151:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 152:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 153:
                         { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 154:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 155:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 156:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 157:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 158:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1103_TRANSPOSE_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 159:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 160:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1104_CARET_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 161:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 162:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 163:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 164:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 165:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 166:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 167:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 168:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 169:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 170:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 171:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 172:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 173:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 174:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 175:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 176:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 177:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 178:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 179:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 180:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 181:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 182:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 183:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 184:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 185:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 186:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
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
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 192:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 193:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 194:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 195:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 196:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 197:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 198:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 199:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 200:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 201:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 202:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 203:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 204:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 205:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 206:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 207:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 208:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 209:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 210:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 211:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 212:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 213:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 214:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 215:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 216:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 217:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 218:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 219:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 220:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 221:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 222:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 223:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 224:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 225:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 226:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 227:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 228:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 229:
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
  case 230:
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
  case 231:
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
  case 232:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 233:
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
  case 234:
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
  case 235:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 236:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 237:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 238:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 239:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 240:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 241:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 242:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 243:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 244:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 245:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 246:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 247:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 248:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 249:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 250:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 251:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 252:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 253:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 254:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 255:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 256:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 257:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 258:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 259:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)), 
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 260:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 261:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 262:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ROLE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 263:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_REF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));   // MCAST_REF kept as internal node name
}
    break;
  case 264:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DOMAIN, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 265:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_RAIL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 266:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARTITION, mc_value_link3(
                    mc_value_create_data(MCAST_PARTITION_LEVEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 267:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ERROR, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 268:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 269:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 270:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 271:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 272:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 273:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 274:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 275:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 276:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 277:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 278:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 279:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 280:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES,
                mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 281:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 282:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 283:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 284:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 285:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 286:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 294:
{
    char buf[160];
    snprintf(buf, sizeof buf,
             "Reserved word '%s' cannot be an attribute value (reserved words "
             "are not names anywhere); pick another spelling.", (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring);
    mc_dlog_add(MCD_E1032_TATTR_RESWORD, 1, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, buf);
    ((*yyvalp).value) = NULL;
}
    break;
  case 295:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_FUNCTION, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 296:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 297:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 298:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 299:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 300:
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 301:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 302:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link(
        mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 303:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 304:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 305:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 306:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 307:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 308:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 309:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 310:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 311:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value))))
        );
}
    break;
  case 312:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, 
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))
        );
}
    break;
  case 313:
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
  case 314:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 315:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 316:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 317: /* mc_declare_a1: mc_ids mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 318: /* mc_declare_a1: mc_ids MCPT_DOT mc_int mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 319: /* mc_declare_a1: mc_ids MCPT_LPAREN mc_params MCPT_RPAREN mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 320:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 321:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 322:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 323:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 325:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 326:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value))));
}
    break;
  case 327:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                        mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                        mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-8)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value))))
                    ));
}
    break;
  case 328:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 329:
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 330:
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 331:
                                            { ((*yyvalp).value) = NULL; }
    break;
  case 332:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 333:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 334:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 335:
                                                    { ((*yyvalp).value) = NULL; }
    break;
  case 336:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 337:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 338:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 339:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 340:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 341:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 342:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES,
            mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))));
}
    break;
  case 343:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 344:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES,
            mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))));
}
    break;
  case 345:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
            mc_value_link(
                mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 346:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_IF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 347:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), 
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), 
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 348:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 349:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link4(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 350:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 351:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 352:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 353:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 354:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 355:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 356:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 357:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 358:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_EQEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 359:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_AND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 360:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_OR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 361:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_NOTEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 362:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));}
    break;
  case 363:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATERTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 364:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSEQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 365:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATEREQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 366:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITAND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 367:
                                                { mc_dlog_add(MCD_W1101_SINGLE_OR, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITOR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 368:
                                                { mc_dlog_add(MCD_W1102_PLUSMINUS, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 369:
                                                               { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_IN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))); }
    break;
  case 370:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 371:
          { 
            mc_dlog_add(MCD_E1017_JUDGE_ERROR, 1, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len, NULL);
            ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BARE_LITERAL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 372:
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 373:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 374:
        {
            unsigned int start = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos;
            unsigned int end = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len;
            ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strndup(g_lex_src + start, end - start), start, end - start);
        }
    break;
  case 375:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 376:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 377:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 378:
        {
            // Transient span carrier for the chain; the data field is unused.
            ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strdup(""), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen) - (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos);
        }
    break;
  case 379:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 380:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 381:
                                                     { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 382:
                                                     { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 383:
        { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 482:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 483:
                                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 484:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 485:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 486:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 487:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 488:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 489:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 490:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 491:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 492:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 493:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 494:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 495:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 496:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 497:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 498:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 499:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 500:
                            { ((*yyvalp).value) = mc_value_create_data(MCAST_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 501:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 502:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 503:
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 504:
                       { ((*yyvalp).value) = mc_value_create_data(MCAST_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 505:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 506:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 507:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 508:
                               { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_USCORE, strdup("_"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 509:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 510:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 511:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 512:
                        {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 513:
                                              {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 514:
                                                                    {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 515:
                                       {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 516:
                                   {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 517:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 518:
{
    const char *left = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->data ? (const char*)(YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->data : "";
    const char *right = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->data ? (const char*)(YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->data : "";
    size_t ll = strlen(left);
    size_t lr = strlen(right);
    char *joined = (char*)malloc(ll + 1 + lr + 1);
    if (joined == NULL) {
        ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value);
    } else {
        memcpy(joined, left, ll);
        joined[ll] = '-';
        memcpy(joined + ll + 1, right, lr + 1);
        ((*yyvalp).value) = mc_value_create_data(MCAST_ID, joined, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->pos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)->len - (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)->pos);
    }
}
    break;
  case 519:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 520:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_OUT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 521:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IO, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 522:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSRC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 523:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSNK, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 524:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSBI, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 525:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 526:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 527:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 528:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 529:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 530:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 531:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 532:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 533:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 534:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 535:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 536:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 537:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 538:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 539:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 540:
                  { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 541:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 542:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 543:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 544:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 545:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 546:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 547:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 548:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 549:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 550:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 551:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 552:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 553:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 554:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_COMPOSITE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 555:
        { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 556:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 557:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 558:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 559:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 560:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 561:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 562:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 563:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 564:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 565:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 566:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 567:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 568:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 569:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 570:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 571:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 572:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 573:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 574:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 575:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 576:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 577:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 578:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 579:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 580:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 581:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 582:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 583:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 584:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 585:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 586:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 587:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_DIV, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 588:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_MUL, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 589:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
	}
    break;
  case 590:
                  {}
    break;
  case 591:
                         {}
    break;
  case 592:
                           {}
    break;
  case 593:
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
