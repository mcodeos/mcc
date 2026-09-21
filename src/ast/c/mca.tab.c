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
  YYSYMBOL_MCPT_DBCOLON = 62,              /* MCPT_DBCOLON  */
  YYSYMBOL_MCK_ELSE_IF = 63,               /* MCK_ELSE_IF  */
  YYSYMBOL_MCK_ELSE = 64,                  /* MCK_ELSE  */
  YYSYMBOL_MCK_IF = 65,                    /* MCK_IF  */
  YYSYMBOL_MC_ENDL = 66,                   /* MC_ENDL  */
  YYSYMBOL_MCK_RETURN = 67,                /* MCK_RETURN  */
  YYSYMBOL_MCK_IO = 68,                    /* MCK_IO  */
  YYSYMBOL_MCK_IN = 69,                    /* MCK_IN  */
  YYSYMBOL_MCK_OUT = 70,                   /* MCK_OUT  */
  YYSYMBOL_MCK_ANL = 71,                   /* MCK_ANL  */
  YYSYMBOL_MCK_NC = 72,                    /* MCK_NC  */
  YYSYMBOL_MCK_LABEL = 73,                 /* MCK_LABEL  */
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
  YYSYMBOL_MC_WS = 142,                    /* MC_WS  */
  YYSYMBOL_MC_SINGLE_COMMENT = 143,        /* MC_SINGLE_COMMENT  */
  YYSYMBOL_MC_MULTI_COMMENT = 144,         /* MC_MULTI_COMMENT  */
  YYSYMBOL_YYACCEPT = 145,                 /* $accept  */
  YYSYMBOL_start = 146,                    /* start  */
  YYSYMBOL_mc_tops = 147,                  /* mc_tops  */
  YYSYMBOL_mc_top = 148,                   /* mc_top  */
  YYSYMBOL_mc_use = 149,                   /* mc_use  */
  YYSYMBOL_mc_uri = 150,                   /* mc_uri  */
  YYSYMBOL_mc_prefix = 151,                /* mc_prefix  */
  YYSYMBOL_mc_uri_trunk = 152,             /* mc_uri_trunk  */
  YYSYMBOL_mc_levels = 153,                /* mc_levels  */
  YYSYMBOL_mc_class_name = 154,            /* mc_class_name  */
  YYSYMBOL_mc_component = 155,             /* mc_component  */
  YYSYMBOL_mc_module = 156,                /* mc_module  */
  YYSYMBOL_mc_interface = 157,             /* mc_interface  */
  YYSYMBOL_mc_enum = 158,                  /* mc_enum  */
  YYSYMBOL_mc_define = 159,                /* mc_define  */
  YYSYMBOL_mc_capability = 160,            /* mc_capability  */
  YYSYMBOL_mc_component_derivs = 161,      /* mc_component_derivs  */
  YYSYMBOL_mc_component_variant = 162,     /* mc_component_variant  */
  YYSYMBOL_mc_component_adopts = 163,      /* mc_component_adopts  */
  YYSYMBOL_mc_body = 164,                  /* mc_body  */
  YYSYMBOL_mc_clauses = 165,               /* mc_clauses  */
  YYSYMBOL_mc_clause = 166,                /* mc_clause  */
  YYSYMBOL_mc_attribute = 167,             /* mc_attribute  */
  YYSYMBOL_mc_attr_values = 168,           /* mc_attr_values  */
  YYSYMBOL_mc_attr_value = 169,            /* mc_attr_value  */
  YYSYMBOL_mc_attr_lines = 170,            /* mc_attr_lines  */
  YYSYMBOL_mc_attribute_pin = 171,         /* mc_attribute_pin  */
  YYSYMBOL_mc_pins_lines = 172,            /* mc_pins_lines  */
  YYSYMBOL_mc_pins_line = 173,             /* mc_pins_line  */
  YYSYMBOL_mc_pin_idn = 174,               /* mc_pin_idn  */
  YYSYMBOL_mc_pins_names = 175,            /* mc_pins_names  */
  YYSYMBOL_mc_pins_name = 176,             /* mc_pins_name  */
  YYSYMBOL_mc_net = 177,                   /* mc_net  */
  YYSYMBOL_mc_opds = 178,                  /* mc_opds  */
  YYSYMBOL_mc_net_port_elem = 179,         /* mc_net_port_elem  */
  YYSYMBOL_mc_net_port_elems = 180,        /* mc_net_port_elems  */
  YYSYMBOL_mc_mn_opd = 181,                /* mc_mn_opd  */
  YYSYMBOL_mc_mn_opds = 182,               /* mc_mn_opds  */
  YYSYMBOL_mc_opd = 183,                   /* mc_opd  */
  YYSYMBOL_mc_phrases = 184,               /* mc_phrases  */
  YYSYMBOL_mc_phrase = 185,                /* mc_phrase  */
  YYSYMBOL_mc_role = 186,                  /* mc_role  */
  YYSYMBOL_mc_conduit = 187,               /* mc_conduit  */
  YYSYMBOL_mc_domain = 188,                /* mc_domain  */
  YYSYMBOL_mc_rail = 189,                  /* mc_rail  */
  YYSYMBOL_mc_partition = 190,             /* mc_partition  */
  YYSYMBOL_mc_tattrs = 191,                /* mc_tattrs  */
  YYSYMBOL_mc_tattrs_opt = 192,            /* mc_tattrs_opt  */
  YYSYMBOL_mc_tattr = 193,                 /* mc_tattr  */
  YYSYMBOL_mc_tkey = 194,                  /* mc_tkey  */
  YYSYMBOL_mc_function = 195,              /* mc_function  */
  YYSYMBOL_mc_paramds = 196,               /* mc_paramds  */
  YYSYMBOL_mc_pards = 197,                 /* mc_pards  */
  YYSYMBOL_mc_pard = 198,                  /* mc_pard  */
  YYSYMBOL_mc_declare_a = 199,             /* mc_declare_a  */
  YYSYMBOL_mc_declare_a1 = 200,            /* mc_declare_a1  */
  YYSYMBOL_mc_insts = 201,                 /* mc_insts  */
  YYSYMBOL_mc_inst = 202,                  /* mc_inst  */
  YYSYMBOL_mc_iface_name = 203,            /* mc_iface_name  */
  YYSYMBOL_mc_declare_b = 204,             /* mc_declare_b  */
  YYSYMBOL_mc_params = 205,                /* mc_params  */
  YYSYMBOL_mc_param = 206,                 /* mc_param  */
  YYSYMBOL_mc_conds = 207,                 /* mc_conds  */
  YYSYMBOL_mc_conds_elifs = 208,           /* mc_conds_elifs  */
  YYSYMBOL_mc_cond_block = 209,            /* mc_cond_block  */
  YYSYMBOL_mc_expr = 210,                  /* mc_expr  */
  YYSYMBOL_mc_judge = 211,                 /* mc_judge  */
  YYSYMBOL_mc_id = 212,                    /* mc_id  */
  YYSYMBOL_mc_ida = 213,                   /* mc_ida  */
  YYSYMBOL_mc_idss = 214,                  /* mc_idss  */
  YYSYMBOL_mc_ids = 215,                   /* mc_ids  */
  YYSYMBOL_mc_idseg = 216,                 /* mc_idseg  */
  YYSYMBOL_mc_idm = 217,                   /* mc_idm  */
  YYSYMBOL_mc_idans = 218,                 /* mc_idans  */
  YYSYMBOL_mc_idan = 219,                  /* mc_idan  */
  YYSYMBOL_mc_int = 220,                   /* mc_int  */
  YYSYMBOL_mc_hex = 221,                   /* mc_hex  */
  YYSYMBOL_mc_float = 222,                 /* mc_float  */
  YYSYMBOL_mc_number = 223,                /* mc_number  */
  YYSYMBOL_mc_string = 224,                /* mc_string  */
  YYSYMBOL_mc_const = 225,                 /* mc_const  */
  YYSYMBOL_mc_nc = 226,                    /* mc_nc  */
  YYSYMBOL_mc_underscore = 227,            /* mc_underscore  */
  YYSYMBOL_mc_literal = 228,               /* mc_literal  */
  YYSYMBOL_mc_iotype = 229,                /* mc_iotype  */
  YYSYMBOL_mc_unit_value = 230,            /* mc_unit_value  */
  YYSYMBOL_mc_unit_type = 231,             /* mc_unit_type  */
  YYSYMBOL_mc_endls = 232                  /* mc_endls  */
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
#define YYLAST   3657
/* YYNTOKENS -- Number of terminals.  */
#define YYNTOKENS  145
/* YYNNTS -- Number of nonterminals.  */
#define YYNNTS  88
/* YYNRULES -- Number of rules.  */
#define YYNRULES  434
/* YYNSTATES -- Number of states.  */
#define YYNSTATES  727
/* YYMAXRHS -- Maximum number of symbols on right-hand side of rule.  */
#define YYMAXRHS 9
/* YYMAXLEFT -- Maximum number of symbols to the left of a handle
   accessed by $0, $-1, etc., in any rule.  */
#define YYMAXLEFT 0
/* YYMAXUTOK -- Last valid token kind.  */
#define YYMAXUTOK   399
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
     135,   136,   137,   138,   139,   140,   141,   142,   143,   144
};
#if MCA_DEBUG
/* YYRLINE[YYN] -- source line where rule number YYN was defined.  */
static const yytype_int16 yyrline[] =
{
       0,   163,   163,   164,   165,   166,   167,   169,   170,   172,
     173,   174,   175,   176,   177,   178,   179,   187,   192,   206,
     211,   230,   238,   248,   254,   262,   266,   276,   277,   278,
     280,   284,   289,   294,   299,   305,   316,   332,   333,   334,
     335,   336,   337,   341,   342,   346,   356,   369,   379,   389,
     398,   404,   412,   415,   417,   419,   421,   424,   437,   438,
     439,   441,   442,   443,   444,   445,   446,   447,   448,   449,
     450,   451,   454,   473,   484,   489,   496,   500,   504,   508,
     514,   515,   519,   529,   539,   552,   553,   554,   556,   563,
     571,   579,   589,   601,   610,   621,   632,   642,   647,   652,
     657,   662,   669,   673,   678,   683,   691,   696,   701,   706,
     715,   728,   729,   743,   744,   745,   746,   762,   763,   764,
     769,   770,   773,   778,   783,   789,   795,   803,   815,   821,
     827,   835,   844,   849,   856,   861,   866,   871,   872,   873,
     878,   879,   881,   882,   883,   884,   886,   887,   888,   889,
     890,   891,   892,   893,   894,   896,   897,   898,   899,   900,
     901,   902,   903,   904,   906,   907,   908,   909,   910,   911,
     912,   913,   914,   916,   917,   918,   919,   920,   921,   922,
     923,   924,   926,   927,   928,   929,   930,   931,   932,   933,
     934,   936,   937,   938,   939,   940,   941,   942,   943,   944,
     946,   947,   948,   949,   951,   952,   953,   954,   956,   961,
     966,   971,   990,  1001,  1010,  1015,  1022,  1030,  1040,  1050,
    1059,  1068,  1076,  1084,  1092,  1102,  1111,  1122,  1133,  1143,
    1152,  1161,  1170,  1204,  1211,  1220,  1227,  1238,  1243,  1251,
    1265,  1271,  1277,  1294,  1311,  1316,  1323,  1326,  1331,  1336,
    1347,  1351,  1355,  1362,  1374,  1375,  1376,  1378,  1379,  1383,
    1387,  1395,  1401,  1408,  1414,  1420,  1426,  1432,  1438,  1444,
    1450,  1456,  1466,  1475,  1491,  1499,  1508,  1516,  1525,  1530,
    1537,  1542,  1552,  1553,  1554,  1561,  1569,  1579,  1580,  1581,
    1584,  1588,  1592,  1596,  1604,  1611,  1623,  1634,  1646,  1651,
    1660,  1665,  1677,  1682,  1689,  1694,  1699,  1706,  1707,  1708,
    1710,  1711,  1712,  1713,  1714,  1715,  1716,  1717,  1718,  1719,
    1720,  1721,  1722,  1727,  1728,  1729,  1731,  1732,  1733,  1736,
    1737,  1738,  1739,  1740,  1741,  1742,  1743,  1744,  1745,  1746,
    1747,  1749,  1750,  1751,  1752,  1753,  1754,  1756,  1758,  1759,
    1760,  1761,  1766,  1767,  1768,  1769,  1773,  1777,  1781,  1787,
    1788,  1789,  1790,  1791,  1792,  1793,  1794,  1795,  1802,  1803,
    1804,  1805,  1806,  1807,  1808,  1809,  1810,  1811,  1812,  1813,
    1814,  1815,  1816,  1817,  1818,  1819,  1820,  1821,  1822,  1823,
    1824,  1825,  1826,  1827,  1828,  1829,  1833,  1834,  1835,  1836,
    1837,  1838,  1839,  1840,  1841,  1842,  1843,  1844,  1845,  1846,
    1847,  1848,  1849,  1850,  1851,  1852,  1853,  1854,  1855,  1856,
    1857,  1858,  1859,  1860,  1861,  1862,  1863,  1864,  1865,  1870,
    1875,  1881,  1882,  1883,  1884
};
#endif
#define YYPACT_NINF (-580)
#define YYTABLE_NINF (-345)
/* YYPACT[STATE-NUM] -- Index in YYTABLE of the portion describing
   STATE-NUM.  */
static const yytype_int16 yypact[] =
{
     933,  -580,    34,    64,   618,   618,   618,   618,   618,   618,
     114,  -580,  -580,   192,    56,  -580,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,   611,   226,  -580,  -580,    11,  -580,   196,
     205,   218,  1047,  -580,  -580,   182,  -580,  -580,   228,   469,
     182,   182,   224,   237,   237,   618,  -580,  1057,  -580,  -580,
      56,  -580,   265,   241,  -580,   618,   267,   618,  -580,   205,
     635,  -580,   205,   205,   205,   205,   368,   278,   305,   618,
     478,  -580,   237,   237,   618,  2003,  -580,  -580,   182,  -580,
    1187,   618,  -580,   304,  -580,   618,  -580,  -580,   352,   362,
    -580,  -580,  -580,  -580,  -580,   399,   549,    21,  2845,   181,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
     581,   283,    10,  -580,  -580,  -580,   145,  -580,  2845,   618,
     237,   331,  -580,  -580,  -580,  -580,   508,  -580,   382,  -580,
    -580,   545,  -580,  -580,  -580,  -580,   618,   618,   618,  2845,
     618,   520,  2845,  2845,  3460,  2984,  2845,  -580,  -580,   205,
     582,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,    20,
    -580,  -580,  -580,  -580,  1671,  1465,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,   377,   190,  -580,  -580,
    -580,  -580,  -580,  -580,   321,  2845,   472,   278,   304,   618,
    -580,  -580,  -580,   618,   478,   467,  2845,  -580,  1671,    82,
    2002,   245,   321,   442,   522,   478,   536,  1627,   618,   815,
    -580,   305,  3430,  1627,  3544,  -580,   475,  2002,   103,  -580,
    -580,   618,  -580,  -580,  -580,   478,   305,  -580,   540,   556,
     556,  2002,   237,   478,   478,   528,   558,   143,  1545,  2463,
    -580,  -580,  2984,  1283,  2143,   699,  1862,   321,   548,   182,
     290,  -580,  2285,  2845,   205,   290,   441,  2845,  2845,  2845,
    2845,  2845,  2845,  -580,  -580,  2845,  2845,   478,   164,  3123,
     458,  2845,  2845,  2845,  2845,  2845,  2845,  -580,  -580,  2845,
     556,  -580,  -580,  1627,  3262,  1627,   592,   606,  2845,  2845,
    2845,  2845,  2845,  2845,  -580,    25,  1671,  1465,   336,  1975,
     237,  -580,  -580,  -580,   305,   147,  2845,   597,  -580,  -580,
     305,  -580,   587,  -580,  1671,  2002,    92,  -580,   255,  -580,
     321,   609,   659,  -580,  -580,  -580,  -580,   137,  3544,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,
    -580,   526,  1627,   304,  -580,  -580,  -580,  2003,  -580,   237,
    -580,   642,   596,  2426,  2426,   597,  2845,  2845,  -580,  1228,
    2463,   424,   631,  2845,  2845,  2845,  2845,  2845,  2845,  2845,
    2845,  2845,  2984,  2984,  -580,  -580,   601,  2285,   237,  2845,
    -580,    99,  -580,   605,   546,  -580,   639,   403,   620,   625,
     298,  -580,   181,  2845,  -580,   351,   591,   591,   495,   702,
     495,   702,   495,   702,    62,   495,   702,    62,   247,   307,
    -580,   247,   307,  -580,  1367,  2283,   359,   639,   403,   620,
     632,  -580,  -580,  -580,  -580,   637,  -580,  2845,   366,  1671,
    1801,   490,   181,  -580,   465,   591,   591,   495,   702,   495,
     702,   495,   702,    62,   495,   702,    62,   247,   307,  -580,
     247,   307,  -580,  1367,  2283,   359,  -580,   169,  3401,   652,
    -580,  1671,  2002,   321,   186,   618,   618,   639,   403,   620,
     495,   702,    62,   495,   702,    62,   247,   307,  -580,   247,
     307,  -580,  1367,  2283,   359,    41,  -580,  -580,  -580,  -580,
    -580,  -580,  1671,  2002,   321,   618,  -580,   478,  1627,   626,
    3262,  3262,  1627,   305,   618,    77,  2845,  3544,  3544,   221,
      57,  -580,   640,   664,  -580,   151,  -580,   665,  2002,   666,
     669,  2845,   231,   361,   364,  -580,  2845,  1757,  -580,  -580,
    -580,  -580,  -580,  -580,  -580,  -580,  -580,  -580,   654,  2984,
    2144,   622,  -580,  -580,   308,   290,   290,  1627,   290,   516,
    -580,   314,  1627,  -580,  -580,  1627,  3262,   326,   290,  -580,
     518,  1627,  -580,  -580,  -580,   595,  -580,   238,   376,  3262,
     618,  -580,  -580,  -580,   283,  -580,   648,   621,  -580,   618,
    -580,  -580,   426,  -580,  -580,  -580,  2002,   321,  -580,  -580,
    -580,  -580,  2426,  3262,  -580,  2567,  2706,   673,  -580,  -580,
    -580,   373,  1862,  -580,  2984,  2144,  -580,  -580,   624,   445,
     627,   181,   237,   597,   450,   461,   466,     6,   628,   237,
     470,  -580,  -580,   248,  -580,   592,   606,  1627,   675,   653,
    -580,   294,   652,  -580,   258,  -580,  1671,  2002,   459,  -580,
    2706,  -580,  -580,  1862,  -580,  -580,  -580,  -580,  -580,  -580,
     626,  -580,  -580,  -580,  -580,   626,   688,  -580,   680,   493,
    3262,  1627,  -580,  3262,  2706,   303,   443,  -580,  -580,  -580,
    -580,   478,  -580,  -580,   502,   652,  -580,  3262,  3262,   512,
     630,  -580,   652,   652,  3262,   664,   652
};
/* YYDEFACT[STATE-NUM] -- Default reduction number in state STATE-NUM.
   Performed when YYTABLE does not specify something else to do.  Zero
   means the default is an error.  */
static const yytype_int16 yydefact[] =
{
       0,    16,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   432,   431,     0,     2,     8,     9,    10,    11,    12,
      13,    14,    15,     0,     0,    21,   323,     0,    27,    19,
       0,    23,     0,    42,   324,   256,   325,   330,    44,   329,
     256,   256,     0,     0,     0,     0,     1,     0,   434,   433,
       4,    22,    17,     0,    28,     0,    25,     0,    36,     0,
       0,    33,     0,     0,     0,     0,     0,    54,     0,     0,
       0,   331,     0,     0,     0,     0,    50,    51,   256,     7,
       0,     0,    29,    20,   328,     0,    24,    37,    32,    31,
      40,    39,    41,    38,   351,   259,   128,     0,     0,     0,
     255,   361,   359,   360,   365,   366,   367,   362,   363,   364,
     124,     0,     0,   258,   269,   270,   261,   132,     0,     0,
       0,    56,   341,    43,   332,   338,     0,   336,   339,    47,
      48,     0,    71,   342,   343,   347,     0,     0,     0,     0,
       0,   128,     0,     0,     0,     0,     0,   348,   349,     0,
     124,   368,   369,   370,   371,   372,   373,   374,   375,   376,
     377,   378,   379,   380,   381,   382,   383,   384,   385,   386,
     387,   388,   389,   390,   391,   392,   393,   394,   395,     0,
      59,    61,    62,    63,     0,   246,    64,    67,    68,    69,
      70,    65,   108,   140,   141,    66,   325,   122,   344,   345,
     346,   352,   353,   354,     0,     0,   355,    54,    18,   327,
      26,    35,    34,     0,     0,   129,     0,   264,   138,     0,
     137,   122,   139,     0,   122,     0,   125,   289,     0,     0,
     254,     0,     0,   289,     0,   276,   280,   267,   122,    53,
      45,     0,    52,   333,   334,     0,     0,    49,     0,   246,
     246,   242,     0,     0,     0,     0,     0,     0,     0,     0,
     358,   357,     0,   308,   309,     0,     0,   307,   110,   256,
       0,    57,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   144,   142,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   145,   143,     0,
     247,   109,   244,   289,     0,   289,   274,   276,     0,     0,
       0,     0,     0,     0,   116,   246,   113,   246,   141,     0,
       0,   326,   260,   130,     0,     0,     0,   265,   133,   126,
       0,   292,   128,   350,   293,   294,     0,   288,   122,   291,
     290,     0,   282,   257,   123,   262,   263,     0,     0,   396,
     397,   398,   399,   400,   401,   402,   403,   404,   410,   411,
     405,   406,   407,   408,   409,   412,   413,   414,   415,   416,
     417,   418,   419,   420,   421,   422,   423,   424,   425,   426,
     427,   271,   289,    55,   335,   337,   340,     0,   240,     0,
     243,   325,     0,     0,     0,   214,     0,     0,   215,   308,
     309,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   306,   304,   298,     0,     0,     0,
     121,     0,   117,   330,   118,    58,   191,   197,   192,     0,
       0,   118,     0,     0,   233,     0,   221,   223,   204,   206,
     200,   202,   146,   152,   147,   155,   161,   156,   164,   170,
     165,   173,   179,   174,   182,   188,   183,   195,   199,   196,
     325,   209,   208,   251,   252,   248,   250,     0,     0,   117,
       0,   344,     0,   235,     0,   222,   224,   205,   207,   201,
     203,   150,   154,   151,   159,   163,   160,   168,   172,   169,
     177,   181,   178,   186,   190,   187,   245,     0,     0,    72,
      75,    77,    78,    76,     0,     0,     0,   193,   198,   194,
     148,   153,   149,   157,   162,   158,   166,   171,   167,   175,
     180,   176,   184,   189,   185,     0,   106,   107,   356,    46,
     131,   266,   136,   134,   135,     0,   127,     0,     0,     0,
       0,     0,   289,     0,     0,     0,     0,     0,     0,     0,
       0,   241,     0,   334,    96,     0,    86,     0,    99,   330,
     344,     0,     0,     0,     0,   322,     0,   308,   310,   313,
     314,   315,   316,   317,   318,   319,   320,   311,   312,     0,
       0,   300,   305,   253,     0,     0,     0,   289,     0,     0,
     112,     0,   289,   229,   231,   289,     0,     0,     0,   210,
       0,   289,   230,   232,   216,   128,    81,     0,   122,     0,
       0,   278,   279,   115,   113,   114,     0,     0,   287,     0,
     296,   295,     0,   283,   277,   430,   273,   272,   429,   428,
     281,   239,     0,     0,    82,     0,     0,     0,    83,   237,
     238,     0,     0,   299,     0,     0,   119,   120,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   214,     0,     0,
       0,   225,    79,     0,    74,   275,   277,   289,   334,     0,
     284,     0,    73,    85,    88,   101,   104,   105,   103,   102,
       0,   321,   302,     0,   301,   211,   217,   212,   111,   234,
     219,   218,   249,   213,   236,   220,     0,    80,     0,     0,
       0,   289,    84,     0,     0,    92,    90,   303,   226,   227,
     228,     0,   286,   297,     0,    89,   100,     0,     0,    93,
       0,   285,    94,    91,     0,     0,    95
};
/* YYPGOTO[NTERM-NUM].  */
static const yytype_int16 yypgoto[] =
{
    -580,  -580,   695,    72,  -580,   696,  -580,   689,  -580,   136,
    -580,  -580,  -580,  -580,  -580,  -580,   514,  -580,  -580,    30,
     335,   -60,  -479,  -548,  -505,  -580,  -580,  -380,    88,   165,
      50,    27,  -580,   260,   210,  -580,   157,  -258,   500,   -53,
     723,  -580,  -580,  -580,  -580,  -580,  -550,  -210,  -288,  -580,
    -580,    -8,  -580,   523,  -580,   -43,   133,  -188,  -511,   -46,
    -205,   211,  -580,  -580,  -536,   183,  -246,    -3,   207,   -56,
      83,  -580,   -29,  -247,   513,   907,  -580,  -580,   615,  -580,
    -580,  -579,  -580,  1242,   -55,  -131,  -311,     8
};
/* YYDEFGOTO[NTERM-NUM].  */
static const yytype_int16 yydefgoto[] =
{
       0,    13,    14,    15,    16,    29,    30,    31,    32,    35,
      17,    18,    19,    20,    21,    22,   120,   121,   242,   414,
     179,   415,   181,   499,   500,   607,   182,   555,   556,   557,
     674,   675,   183,   589,   314,   315,   420,   421,   184,   257,
     264,   186,   187,   188,   189,   190,   300,   301,   302,   465,
     191,    67,   112,   113,   192,   193,   306,   235,   341,   194,
     336,   337,   195,   581,   416,   265,   266,   196,    37,    83,
     221,    39,   215,   126,   127,   198,   199,   200,   201,   202,
     203,   339,   117,   204,   205,   206,   381,   417
};
/* YYTABLE[YYPACT[STATE-NUM]] -- What to do in state STATE-NUM.  If
   positive, shift that token.  If negative, reduce the rule whose
   number is the opposite.  If YYTABLE_NINF, syntax error.  */
static const yytype_int16 yytable[] =
{
      33,    36,    36,    36,    36,    36,    36,   392,    23,   307,
      71,   118,   496,   261,   562,   180,   401,   430,   131,   606,
     115,    33,    47,   114,   616,   208,    53,    33,   347,    26,
      34,   468,    72,    73,  -119,   620,   621,   545,   229,   388,
     389,   288,    36,   271,   643,   219,    24,    11,   656,    26,
      34,    94,    36,   525,    36,  -119,    87,   679,    80,    90,
      91,    92,    93,    36,    54,    25,    36,    36,   535,   230,
     207,    36,    26,    76,    77,   308,    96,   216,    36,    27,
     631,   226,    36,    11,    11,   672,    12,    38,    38,    38,
      38,    38,    38,   268,    36,   243,    36,   433,   497,    99,
     504,   679,   129,   130,   664,   526,   682,   527,   669,   684,
     326,    26,    34,    36,   311,   312,    36,    28,   231,    79,
     538,   226,    12,    12,   705,   679,   110,   585,    38,   547,
     548,  -268,    45,    36,    36,    36,   625,    36,    84,   327,
      86,    40,    41,    42,    43,    44,   269,   707,   586,   116,
     240,   539,    79,    26,    34,   715,   719,    84,   197,   318,
     231,   233,  -268,   325,    84,   538,   577,   578,   210,   722,
     723,   326,    26,    34,   118,   326,   726,   549,    11,   232,
     217,    78,   224,   115,   697,   383,   114,   272,   528,    26,
      34,    94,    46,   463,    36,   713,   544,   538,    26,    34,
     395,   238,    38,   233,   531,   231,    36,   234,   634,    55,
      36,    36,   425,    26,   538,   384,    96,    12,    36,   248,
     249,   250,    36,   252,   304,    36,    36,    51,   604,    36,
     437,   464,    57,   436,    26,    36,   628,   629,    36,    99,
      66,    27,    36,    68,   476,   610,    74,   475,   305,   538,
     391,    36,   671,    26,    34,   239,    26,    34,    11,    75,
     231,   418,   274,    26,    34,    11,   110,    36,   540,   275,
     231,   429,    36,    36,   288,    48,   124,   125,    81,    28,
     630,    85,   390,   696,   460,    36,   703,    36,   638,   541,
     617,   119,   321,   122,    82,   662,   322,    12,    26,    34,
      94,   283,   284,   233,    12,   227,   434,   704,   122,   228,
     338,   342,   116,   233,    49,   345,   338,   611,   612,   288,
     473,    11,   287,   236,    84,    96,   585,   180,   648,   289,
     650,   717,   209,   642,   308,    36,   326,   622,   561,   561,
     658,   227,   326,   563,   564,   228,   419,   588,    99,   197,
     529,   702,  -114,   224,   326,   197,   624,   582,   224,  -114,
      12,   297,   298,  -114,  -114,   646,   584,   309,   310,   211,
     461,   653,   308,   311,   312,   110,    26,    34,    94,   212,
     591,   313,   649,   657,    26,    34,   338,   654,   338,   326,
     655,   231,   326,   241,   585,   246,   660,    95,   683,  -114,
    -114,   326,  -114,    96,   236,   309,   310,   593,   594,   592,
     304,   311,   312,   228,   597,   598,    97,   496,   287,   551,
     639,   124,   666,   640,    98,   289,    99,   100,   236,    36,
     681,   496,   124,   213,   233,   303,   101,   102,   103,   104,
     105,   106,   107,   108,   109,   236,   602,   603,   583,    26,
      34,    94,   125,   110,   538,   295,   296,   297,   298,   288,
     124,   125,   699,    75,   720,   338,    26,    34,    94,    36,
     197,   718,  -344,   538,   412,   413,    96,   423,   538,   615,
      75,   122,   324,   565,    69,   670,    26,    34,   319,   538,
     432,    70,   704,    96,   609,   466,   714,   433,   538,    99,
     197,   328,    36,    36,   686,  -344,  -344,   472,   273,   690,
     274,  -344,  -344,   641,   433,   224,    99,   275,  -118,  -344,
     691,   538,    36,   601,   384,   692,   110,   228,   288,   695,
     538,   244,    36,   382,    36,   253,   245,   231,   276,  -118,
     724,    36,   254,   110,   651,   236,   651,   281,   282,   283,
     284,   330,   712,   227,   255,   224,   256,   228,   272,   246,
     546,   721,   387,   635,   214,   652,   111,   659,   247,  -339,
     635,    70,   288,   209,  -118,   661,   326,   561,   547,   548,
     561,   608,    36,    36,   393,    36,   568,   569,   570,   571,
     572,   573,   574,   575,   576,    36,   225,   225,   218,   223,
     559,   559,   214,    70,   270,    36,    69,    36,   224,   537,
     214,    -6,     1,    70,   394,   663,    36,   254,   342,   553,
     505,   338,     2,     3,   245,   338,    26,    34,  -338,     4,
       5,     6,     7,  -330,   506,     8,     9,    10,    48,   384,
      88,    89,   218,   258,   668,   263,   218,   685,    36,   245,
     687,   693,   585,   725,   274,   585,   585,   708,   245,   535,
      36,   275,   709,   197,   579,   580,   710,   542,   224,   224,
     338,   224,   311,   312,   543,   338,   552,    49,   338,   635,
     609,   224,   689,   587,   338,   644,   645,   566,   619,   694,
     595,   281,   282,   283,   284,   596,   632,   227,   633,   636,
     -98,   228,   342,   -97,   412,   316,   667,   680,    36,   700,
     711,   701,   236,   236,   304,   286,   218,   287,    50,    56,
      52,   320,   550,   673,   289,   197,   637,   334,   197,   111,
     706,   716,   600,   334,   224,   613,   403,   404,   405,   406,
     407,   408,   647,   665,   125,   290,   698,   409,   410,   618,
     338,   236,   343,     0,   295,   296,   297,   298,   385,   260,
     411,     0,   399,     0,     0,     0,   197,     0,   559,     0,
     422,     0,     0,   426,     0,   422,   435,   438,   440,   442,
     445,   448,   451,     0,   338,   454,   457,     0,     0,   469,
     474,   477,   479,   481,   484,   487,   490,     0,   185,   493,
       0,     0,     0,   334,   501,   334,     0,     0,   507,   510,
     513,   516,   519,   522,     0,   236,     0,   236,     0,     0,
       0,   220,     0,    26,    34,    94,   532,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   559,
       0,   237,   559,     0,    95,     0,     0,     0,     0,     0,
      96,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   251,    97,     0,   220,   259,     0,     0,   220,
       0,    98,     0,    99,     0,     0,     0,     0,     0,     0,
       0,     0,   334,   101,   102,   103,   104,   105,   106,   107,
     108,   109,     0,     0,     0,     0,   218,   218,     0,     0,
     110,     0,     0,   567,   567,   567,   567,   567,   567,   567,
     567,   567,   263,   263,     0,     0,     0,     0,   125,   218,
       0,     0,     0,     0,     0,     0,     0,     0,   317,     0,
       0,     0,   590,   218,     1,     0,     0,     0,     0,   220,
       0,     0,     0,     0,     2,     3,     0,     0,     0,     0,
     335,     4,     5,     6,     7,     0,   335,     8,     9,    10,
      11,     0,     0,     0,     0,     0,     0,   218,     0,     0,
       0,     0,   590,     0,     0,   123,     0,   128,     0,     0,
       0,     0,     0,     0,     0,   400,     0,     0,     0,   185,
       0,     0,     0,     0,     0,   185,   427,     0,   218,    12,
     439,   441,   443,   446,   449,   452,     0,     0,   455,   458,
       0,     0,   470,     0,   478,   480,   482,   485,   488,   491,
       0,     0,   494,     0,     0,   614,   335,   502,   335,     0,
       0,   508,   511,   514,   517,   520,   523,     0,   334,     0,
     501,   501,   334,     0,     0,     0,     0,   -30,    58,   533,
       0,     0,     0,     0,     0,     0,     0,    -3,     1,     0,
     -30,   -30,    59,    60,    61,     0,   218,     0,     2,     3,
       0,     0,     0,     0,   -30,     4,     5,     6,     7,   263,
       0,     8,     9,    10,    48,   422,   422,   334,   422,     0,
       0,     0,   334,    62,    63,   334,   501,     0,   422,    64,
      65,   334,     0,     0,     0,   335,     0,     0,     0,   501,
     185,     0,     0,   -30,     0,     0,   558,   558,     0,   220,
     220,   323,     0,    49,     0,     0,     0,     0,     0,     0,
       0,     0,   329,   501,     0,     0,   676,     0,   344,     0,
     185,     0,   220,     0,   263,     0,     0,     0,     0,     0,
       0,   688,   128,   386,     0,     0,   220,     0,     0,     0,
     323,   128,     0,     0,     0,     0,     0,   334,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   424,     0,     0,
     676,     0,   431,     0,     0,     0,     0,    -5,     1,     0,
     220,     0,     0,     0,   462,     0,   471,     0,     2,     3,
     501,   334,     0,   501,   676,     4,     5,     6,     7,     0,
       0,     8,     9,    10,    48,     0,     0,   501,   501,     0,
       0,   220,     0,     0,   501,     0,     0,     0,     0,     0,
       0,   530,     0,     0,     0,     0,     0,   536,     0,     0,
       0,   273,     0,   274,     0,     0,     0,     0,     0,     0,
     275,     0,     0,    49,     0,     0,   396,     0,     0,     0,
       0,   335,     0,   502,   502,   335,     0,     0,     0,   626,
       0,   276,   277,   278,   279,   280,     0,     0,     0,     0,
     281,   282,   283,   284,   558,     0,   227,   328,   285,   220,
     228,     0,     0,     0,     0,     0,   273,   402,   274,     0,
     560,   560,     0,   185,     0,   275,     0,     0,     0,     0,
     335,     0,     0,     0,     0,   335,     0,     0,   335,   502,
       0,     0,     0,     0,   335,     0,   276,   277,   278,   279,
     280,     0,   502,     0,     0,   281,   282,   283,   284,     0,
     222,   227,     0,   285,     0,   228,     0,     0,     0,     0,
       0,     0,   402,     0,     0,   558,   502,     0,   558,   677,
       0,     0,     0,     0,     0,   185,     0,     0,   185,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     273,     0,   274,     0,   222,     0,     0,   267,   222,   275,
     335,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   677,     0,     0,   185,     0,     0,     0,
     276,   277,   278,   279,   280,     0,     0,     0,     0,   281,
     282,   283,   284,   502,   335,   227,   502,   677,     0,   228,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     502,   502,     0,     0,   128,     0,     0,   502,     0,     0,
     623,     0,     0,     0,     0,     0,     0,     0,   222,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   560,   340,
       0,     0,     0,     0,   346,   340,     0,     0,   286,     0,
     287,   288,     0,     0,     0,     0,     0,   289,     0,     0,
       0,     0,   431,   431,     0,   431,     0,     0,     0,     0,
       0,     0,     0,     0,   267,   431,     0,     0,   290,   291,
     292,   293,   294,     0,     0,   428,     0,   295,   296,   297,
     298,   444,   447,   450,   453,   299,     0,   456,   459,     0,
       0,     0,     0,     0,     0,   483,   486,   489,   492,   560,
       0,   495,   560,   678,     0,   340,   503,   340,     0,     0,
     509,   512,   515,   518,   521,   524,     0,     0,   273,     0,
     274,     0,     0,     0,     0,     0,     0,   275,   534,     0,
       0,     0,     0,   396,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   678,   276,   277,
     278,   279,   280,     0,     0,     0,     0,   281,   282,   283,
     284,     0,     0,   227,   328,   285,     0,   228,     0,     0,
       0,   678,     0,     0,     0,     0,     0,     0,   128,     0,
       0,     0,     0,     0,   340,     0,     0,     0,     0,     0,
     122,   133,   134,     0,   135,    26,    34,    94,   222,   222,
       0,     0,     0,     0,     0,   267,   267,   267,   267,   267,
     267,   267,   267,   267,   267,   267,   331,     0,     0,     0,
       0,   222,   332,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,   222,     0,     0,     0,     0,
       0,     0,     0,   142,   273,   143,   274,     0,   144,     0,
       0,     0,     0,   275,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   147,   148,   333,     0,     0,   222,
       0,     0,   150,     0,   276,   277,   278,   279,   280,     0,
       0,     0,     0,   281,   282,   283,   284,     0,     0,   227,
       0,   285,     0,   228,     0,     0,     0,     0,     0,     0,
     222,   151,   152,   153,   154,   155,   156,   157,   158,   159,
     160,   161,   162,   163,   164,   165,   166,   167,   168,   169,
     170,   171,   172,   173,   174,   175,   176,   177,   178,     0,
     273,     0,   274,     0,     0,     0,     0,     0,     0,   275,
     340,     0,   503,   503,   340,     0,     0,     0,   627,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     276,   277,   278,   279,   280,     0,     0,     0,   222,   281,
     282,   283,   284,     0,   286,   227,   287,   285,     0,   228,
       0,   267,     0,   289,   599,     0,     0,     0,     0,   340,
       0,     0,     0,     0,   340,     0,     0,   340,   503,     0,
       0,     0,     0,   340,   290,   291,   292,   293,   294,     0,
       0,   503,     0,   295,   296,   297,   298,     0,     0,     0,
       0,   299,     0,   132,     0,   122,   133,   134,     0,   135,
      26,    34,    94,     0,     0,   503,     0,     0,     0,     0,
       0,     0,     0,     0,    75,     0,   267,     0,     0,    11,
       0,   136,   137,   138,   139,   140,     0,   141,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   340,
       0,     0,   412,   413,     0,     0,     0,     0,   142,     0,
     143,     0,     0,   144,     0,     0,     0,   145,    12,   146,
     101,   102,   103,   104,   105,   106,   107,   108,   109,   147,
     148,     0,   503,   340,     0,   503,   149,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   503,
     503,     0,     0,     0,     0,     0,   503,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   132,     0,   122,   133,   134,     0,
     135,    26,    34,    94,     0,   286,     0,   287,     0,     0,
       0,     0,     0,     0,   289,     0,   -60,     0,     0,     0,
     -60,     0,   136,   137,   138,   139,   140,     0,   141,     0,
       0,     0,     0,     0,     0,   290,   291,   292,   293,   294,
       0,     0,     0,     0,   295,   296,   297,   298,     0,   142,
       0,   143,   299,     0,   144,     0,     0,     0,   145,   -60,
     146,   101,   102,   103,   104,   105,   106,   107,   108,   109,
     147,   148,     0,     0,     0,     0,     0,   149,   150,   151,
     152,   153,   154,   155,   156,   157,   158,   159,   160,   161,
     162,   163,   164,   165,   166,   167,   168,   169,   170,   171,
     172,   173,   174,   175,   176,   177,   178,   151,   152,   153,
     154,   155,   156,   157,   158,   159,   160,   161,   162,   163,
     164,   165,   166,   167,   168,   169,   170,   171,   172,   173,
     174,   175,   176,   177,   178,   132,     0,   122,   133,   134,
       0,   135,    26,    34,    94,     0,   286,     0,   287,     0,
       0,     0,     0,     0,     0,   289,    75,     0,     0,     0,
       0,    11,     0,   136,   137,   138,   139,   140,     0,   141,
       0,     0,     0,     0,     0,     0,   290,   291,   292,   293,
     294,     0,     0,     0,     0,   295,   296,   297,   298,     0,
     142,     0,   143,   299,     0,   144,     0,     0,     0,   145,
      12,   146,   101,   102,   103,   104,   105,   106,   107,   108,
     109,   147,   148,     0,     0,     0,     0,     0,   149,   150,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   151,   152,
     153,   154,   155,   156,   157,   158,   159,   160,   161,   162,
     163,   164,   165,   166,   167,   168,   169,   170,   171,   172,
     173,   174,   175,   176,   177,   178,   132,     0,   122,   133,
     134,     0,   135,    26,    34,    94,   286,     0,   287,     0,
       0,     0,     0,     0,     0,   289,     0,     0,     0,     0,
       0,     0,    48,     0,   136,   137,   138,   139,   140,     0,
     141,     0,     0,     0,     0,     0,   290,   291,   292,   293,
     294,     0,     0,     0,     0,   295,   296,   297,   298,     0,
       0,   142,     0,   143,     0,     0,   144,     0,     0,     0,
     145,    49,   146,   101,   102,   103,   104,   105,   106,   107,
     108,   109,   147,   148,     0,     0,     0,     0,     0,   149,
     150,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   151,
     152,   153,   154,   155,   156,   157,   158,   159,   160,   161,
     162,   163,   164,   165,   166,   167,   168,   169,   170,   171,
     172,   173,   174,   175,   176,   177,   178,   554,     0,   122,
     133,   134,     0,   135,    26,    34,    94,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   -87,     0,     0,     0,     0,     0,     0,
       0,    96,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   286,     0,   287,     0,
       0,     0,   142,   -87,   143,   289,     0,   144,     0,     0,
       0,   397,   -87,     0,   101,   102,   103,   104,   105,   106,
     107,   108,   109,   147,   148,     0,   290,   291,   292,   293,
     294,   150,     0,     0,     0,   295,   296,   297,   298,     0,
       0,     0,   398,   299,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     151,   152,   153,   154,   155,   156,   157,   158,   159,   160,
     161,   162,   163,   164,   165,   166,   167,   168,   169,   170,
     171,   172,   173,   174,   175,   176,   177,   178,   554,     0,
     122,   133,   134,     0,   135,    26,    34,    94,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    48,     0,     0,     0,     0,     0,
       0,     0,    96,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   142,     0,   143,     0,     0,   144,     0,
       0,     0,     0,    49,     0,   101,   102,   103,   104,   105,
     106,   107,   108,   109,   147,   148,     0,     0,     0,     0,
       0,     0,   150,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   151,   152,   153,   154,   155,   156,   157,   158,   159,
     160,   161,   162,   163,   164,   165,   166,   167,   168,   169,
     170,   171,   172,   173,   174,   175,   176,   177,   178,   122,
     133,   134,     0,   135,    26,    34,    94,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    96,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   142,     0,   143,     0,     0,   144,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   147,   148,   333,     0,     0,     0,     0,
       0,   150,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     151,   152,   153,   154,   155,   156,   157,   158,   159,   160,
     161,   162,   163,   164,   165,   166,   167,   168,   169,   170,
     171,   172,   173,   174,   175,   176,   177,   178,   122,   133,
     134,     0,   135,    26,    34,    94,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      96,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   142,     0,   143,     0,     0,   144,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,   147,   148,     0,     0,     0,     0,     0,     0,
     150,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   151,
     152,   153,   154,   155,   156,   157,   158,   159,   160,   161,
     162,   163,   164,   165,   166,   167,   168,   169,   170,   171,
     172,   173,   174,   175,   176,   177,   178,   122,   133,   134,
       0,   135,    26,    34,    94,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    96,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     142,     0,   262,     0,     0,   144,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,   147,   148,     0,     0,     0,     0,     0,     0,   150,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   151,   152,
     153,   154,   155,   156,   157,   158,   159,   160,   161,   162,
     163,   164,   165,   166,   167,   168,   169,   170,   171,   172,
     173,   174,   175,   176,   177,   178,   122,   133,   134,     0,
     135,    26,    34,    94,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    96,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   467,
       0,   143,     0,     0,   144,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
     147,   148,     0,     0,     0,     0,     0,     0,   150,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   151,   152,   153,
     154,   155,   156,   157,   158,   159,   160,   161,   162,   163,
     164,   165,   166,   167,   168,   169,   170,   171,   172,   173,
     174,   175,   176,   177,   178,   122,   133,   134,     0,   135,
      26,    34,    94,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,    96,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   498,     0,
     143,     0,     0,   144,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,   147,
     148,     0,     0,     0,     0,     0,     0,   150,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,   151,   152,   153,   154,
     155,   156,   157,   158,   159,   160,   161,   162,   163,   164,
     165,   166,   167,   168,   169,   170,   171,   172,   173,   174,
     175,   176,   177,   178,   122,   133,   134,     0,   135,    26,
      34,    94,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,   122,   133,   134,   605,   135,    26,    34,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   142,     0,   143,
       0,     0,   144,   122,   133,   134,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,   147,   148,
       0,     0,     0,     0,     0,     0,   150,     0,     0,     0,
       0,   144,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,   147,   148,     0,
       0,     0,     0,     0,     0,   151,   152,   153,   154,   155,
     156,   157,   158,   159,   160,   161,   162,   163,   164,   165,
     166,   167,   168,   169,   170,   171,   172,   173,   174,   175,
     176,   177,   178,     0,   151,   152,   153,   154,   155,   156,
     157,   158,   159,   160,   161,   162,   163,   164,   165,   166,
     167,   168,   169,   170,   171,   172,   173,   174,   175,   176,
     177,   178,     0,     0,   151,   152,   153,   154,   155,   156,
     157,   158,   159,   160,   161,   162,   163,   164,   165,   166,
     167,   168,   169,   170,   171,   172,   173,   174,   175,   176,
     177,   178,   348,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,   349,   350,   351,   352,     0,     0,
     353,   354,   355,   356,   357,   358,   359,   360,   361,   362,
     363,   364,   365,   366,   367,   368,   369,   370,   371,   372,
     373,   374,   375,   376,   377,   378,   379,   380
};
static const yytype_int16 yycheck[] =
{
       3,     4,     5,     6,     7,     8,     9,   254,     0,   197,
      39,    66,   300,   144,   394,    75,   262,   275,    74,   498,
      66,    24,    14,    66,   535,    81,    15,    30,   233,     8,
       9,   289,    40,    41,    28,   540,   541,   348,    28,   249,
     250,    16,    45,    23,   580,    98,    12,    27,   596,     8,
       9,    10,    55,    28,    57,    49,    59,   636,    50,    62,
      63,    64,    65,    66,    53,     1,    69,    70,    62,    59,
      78,    74,     8,    43,    44,    13,    35,    56,    81,    15,
      23,   110,    85,    27,    27,   633,    66,     4,     5,     6,
       7,     8,     9,   146,    97,   124,    99,    56,   303,    58,
     305,   680,    72,    73,   609,   315,   642,   317,   619,   645,
      28,     8,     9,   116,    52,    53,   119,    53,    15,    47,
      28,   150,    66,    66,   674,   704,    85,    28,    45,    52,
      53,    28,    18,   136,   137,   138,    59,   140,    55,    57,
      57,     5,     6,     7,     8,     9,   149,   683,    49,    66,
     120,    59,    80,     8,     9,   703,   706,    74,    75,   205,
      15,    58,    59,   216,    81,    28,   412,   413,    85,   717,
     718,    28,     8,     9,   229,    28,   724,   382,    27,    34,
      97,    45,    99,   229,   663,   241,   229,   179,   319,     8,
       9,    10,     0,    29,   197,   700,    59,    28,     8,     9,
      57,   118,   119,    58,    57,    15,   209,    62,    57,    13,
     213,   214,   272,     8,    28,   244,    35,    66,   221,   136,
     137,   138,   225,   140,    34,   228,   229,     1,    59,   232,
     276,    67,    14,   276,     8,   238,   547,   548,   241,    58,
      58,    15,   245,    15,   290,    59,    22,   290,    58,    28,
     253,   254,   632,     8,     9,   119,     8,     9,    27,    22,
      15,   269,    15,     8,     9,    27,    85,   270,    13,    22,
      15,   274,   275,   276,    16,    27,    69,    70,    13,    53,
      59,    14,   252,    35,   287,   288,    28,   290,    57,    34,
     537,    13,   209,     3,    53,    57,   213,    66,     8,     9,
      10,    54,    55,    58,    66,    58,   276,    49,     3,    62,
     227,   228,   229,    58,    66,   232,   233,   505,   506,    16,
     290,    27,    15,   116,   241,    35,    28,   387,   586,    22,
     588,    28,    28,   579,    13,   338,    28,   542,   393,   394,
     598,    58,    28,   396,   397,    62,    56,    49,    58,   266,
     320,    57,    16,   270,    28,   272,   544,   417,   275,    23,
      66,    54,    55,    27,    28,    57,   419,    46,    47,    17,
     287,    57,    13,    52,    53,    85,     8,     9,    10,    17,
     433,    60,   587,    57,     8,     9,   303,   592,   305,    28,
     595,    15,    28,    62,    28,    13,   601,    29,   644,    63,
      64,    28,    66,    35,   197,    46,    47,   436,   437,    58,
      34,    52,    53,    62,   467,    49,    48,   705,    15,   389,
      59,   214,   610,    59,    56,    22,    58,    59,   221,   432,
      57,   719,   225,    34,    58,    58,    68,    69,    70,    71,
      72,    73,    74,    75,    76,   238,   475,   476,   418,     8,
       9,    10,   245,    85,    28,    52,    53,    54,    55,    16,
     253,   254,   667,    22,   711,   382,     8,     9,    10,   472,
     387,    28,    13,    28,    50,    51,    35,   270,    28,   525,
      22,     3,    15,    59,    15,    59,     8,     9,    16,    28,
      49,    22,    49,    35,    28,   288,   701,    56,    28,    58,
     417,    59,   505,   506,    59,    46,    47,    49,    13,    59,
      15,    52,    53,   566,    56,   432,    58,    22,    28,    60,
      59,    28,   525,    58,   553,    59,    85,    62,    16,    59,
      28,    23,   535,    58,   537,    15,    28,    15,    43,    49,
      28,   544,    22,    85,    28,   338,    28,    52,    53,    54,
      55,    15,    59,    58,    34,   472,    36,    62,   550,    13,
      34,    59,    22,   555,    15,    49,    66,    49,    23,    23,
     562,    22,    16,    28,    28,   604,    28,   632,    52,    53,
     635,   498,   585,   586,    56,   588,   403,   404,   405,   406,
     407,   408,   409,   410,   411,   598,    15,    15,    98,    99,
     393,   394,    15,    22,    22,   608,    15,   610,   525,    22,
      15,     0,     1,    22,    56,   607,   619,    22,   535,    23,
      28,   538,    11,    12,    28,   542,     8,     9,    23,    18,
      19,    20,    21,    28,    28,    24,    25,    26,    27,   668,
       5,     6,   142,   143,    23,   145,   146,    23,   651,    28,
      23,    23,    28,    23,    15,    28,    28,   686,    28,    62,
     663,    22,   691,   580,    63,    64,   695,    58,   585,   586,
     587,   588,    52,    53,    15,   592,    34,    66,   595,   671,
      28,   598,   652,    58,   601,    63,    64,    56,    62,   659,
      58,    52,    53,    54,    55,    58,    56,    58,    34,    34,
      34,    62,   619,    34,    50,   205,    58,    34,   711,    34,
      22,    58,   505,   506,    34,    13,   216,    15,    23,    30,
      24,   207,   387,   635,    22,   642,   561,   227,   645,   229,
     680,   704,   472,   233,   651,   525,    37,    38,    39,    40,
      41,    42,   585,   610,   537,    43,   663,    48,    49,   538,
     667,   544,   229,    -1,    52,    53,    54,    55,   245,   144,
      61,    -1,   262,    -1,    -1,    -1,   683,    -1,   561,    -1,
     270,    -1,    -1,   273,    -1,   275,   276,   277,   278,   279,
     280,   281,   282,    -1,   701,   285,   286,    -1,    -1,   289,
     290,   291,   292,   293,   294,   295,   296,    -1,    75,   299,
      -1,    -1,    -1,   303,   304,   305,    -1,    -1,   308,   309,
     310,   311,   312,   313,    -1,   608,    -1,   610,    -1,    -1,
      -1,    98,    -1,     8,     9,    10,   326,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   632,
      -1,   118,   635,    -1,    29,    -1,    -1,    -1,    -1,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   139,    48,    -1,   142,   143,    -1,    -1,   146,
      -1,    56,    -1,    58,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   382,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    -1,    -1,    -1,    -1,   396,   397,    -1,    -1,
      85,    -1,    -1,   403,   404,   405,   406,   407,   408,   409,
     410,   411,   412,   413,    -1,    -1,    -1,    -1,   711,   419,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   205,    -1,
      -1,    -1,   432,   433,     1,    -1,    -1,    -1,    -1,   216,
      -1,    -1,    -1,    -1,    11,    12,    -1,    -1,    -1,    -1,
     227,    18,    19,    20,    21,    -1,   233,    24,    25,    26,
      27,    -1,    -1,    -1,    -1,    -1,    -1,   467,    -1,    -1,
      -1,    -1,   472,    -1,    -1,    68,    -1,    70,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   262,    -1,    -1,    -1,   266,
      -1,    -1,    -1,    -1,    -1,   272,   273,    -1,   498,    66,
     277,   278,   279,   280,   281,   282,    -1,    -1,   285,   286,
      -1,    -1,   289,    -1,   291,   292,   293,   294,   295,   296,
      -1,    -1,   299,    -1,    -1,   525,   303,   304,   305,    -1,
      -1,   308,   309,   310,   311,   312,   313,    -1,   538,    -1,
     540,   541,   542,    -1,    -1,    -1,    -1,     0,     1,   326,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,     0,     1,    -1,
      13,    14,    15,    16,    17,    -1,   566,    -1,    11,    12,
      -1,    -1,    -1,    -1,    27,    18,    19,    20,    21,   579,
      -1,    24,    25,    26,    27,   585,   586,   587,   588,    -1,
      -1,    -1,   592,    46,    47,   595,   596,    -1,   598,    52,
      53,   601,    -1,    -1,    -1,   382,    -1,    -1,    -1,   609,
     387,    -1,    -1,    66,    -1,    -1,   393,   394,    -1,   396,
     397,   214,    -1,    66,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,   225,   633,    -1,    -1,   636,    -1,   231,    -1,
     417,    -1,   419,    -1,   644,    -1,    -1,    -1,    -1,    -1,
      -1,   651,   245,   246,    -1,    -1,   433,    -1,    -1,    -1,
     253,   254,    -1,    -1,    -1,    -1,    -1,   667,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   270,    -1,    -1,
     680,    -1,   275,    -1,    -1,    -1,    -1,     0,     1,    -1,
     467,    -1,    -1,    -1,   287,    -1,   289,    -1,    11,    12,
     700,   701,    -1,   703,   704,    18,    19,    20,    21,    -1,
      -1,    24,    25,    26,    27,    -1,    -1,   717,   718,    -1,
      -1,   498,    -1,    -1,   724,    -1,    -1,    -1,    -1,    -1,
      -1,   324,    -1,    -1,    -1,    -1,    -1,   330,    -1,    -1,
      -1,    13,    -1,    15,    -1,    -1,    -1,    -1,    -1,    -1,
      22,    -1,    -1,    66,    -1,    -1,    28,    -1,    -1,    -1,
      -1,   538,    -1,   540,   541,   542,    -1,    -1,    -1,   546,
      -1,    43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,
      52,    53,    54,    55,   561,    -1,    58,    59,    60,   566,
      62,    -1,    -1,    -1,    -1,    -1,    13,    69,    15,    -1,
     393,   394,    -1,   580,    -1,    22,    -1,    -1,    -1,    -1,
     587,    -1,    -1,    -1,    -1,   592,    -1,    -1,   595,   596,
      -1,    -1,    -1,    -1,   601,    -1,    43,    44,    45,    46,
      47,    -1,   609,    -1,    -1,    52,    53,    54,    55,    -1,
      98,    58,    -1,    60,    -1,    62,    -1,    -1,    -1,    -1,
      -1,    -1,    69,    -1,    -1,   632,   633,    -1,   635,   636,
      -1,    -1,    -1,    -1,    -1,   642,    -1,    -1,   645,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      13,    -1,    15,    -1,   142,    -1,    -1,   145,   146,    22,
     667,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,   680,    -1,    -1,   683,    -1,    -1,    -1,
      43,    44,    45,    46,    47,    -1,    -1,    -1,    -1,    52,
      53,    54,    55,   700,   701,    58,   703,   704,    -1,    62,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     717,   718,    -1,    -1,   537,    -1,    -1,   724,    -1,    -1,
     543,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   216,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   561,   227,
      -1,    -1,    -1,    -1,   232,   233,    -1,    -1,    13,    -1,
      15,    16,    -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,
      -1,    -1,   585,   586,    -1,   588,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,   262,   598,    -1,    -1,    43,    44,
      45,    46,    47,    -1,    -1,   273,    -1,    52,    53,    54,
      55,   279,   280,   281,   282,    60,    -1,   285,   286,    -1,
      -1,    -1,    -1,    -1,    -1,   293,   294,   295,   296,   632,
      -1,   299,   635,   636,    -1,   303,   304,   305,    -1,    -1,
     308,   309,   310,   311,   312,   313,    -1,    -1,    13,    -1,
      15,    -1,    -1,    -1,    -1,    -1,    -1,    22,   326,    -1,
      -1,    -1,    -1,    28,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   680,    43,    44,
      45,    46,    47,    -1,    -1,    -1,    -1,    52,    53,    54,
      55,    -1,    -1,    58,    59,    60,    -1,    62,    -1,    -1,
      -1,   704,    -1,    -1,    -1,    -1,    -1,    -1,   711,    -1,
      -1,    -1,    -1,    -1,   382,    -1,    -1,    -1,    -1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,   396,   397,
      -1,    -1,    -1,    -1,    -1,   403,   404,   405,   406,   407,
     408,   409,   410,   411,   412,   413,    29,    -1,    -1,    -1,
      -1,   419,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,   433,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    13,    58,    15,    -1,    61,    -1,
      -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    77,    78,    79,    -1,    -1,   467,
      -1,    -1,    85,    -1,    43,    44,    45,    46,    47,    -1,
      -1,    -1,    -1,    52,    53,    54,    55,    -1,    -1,    58,
      -1,    60,    -1,    62,    -1,    -1,    -1,    -1,    -1,    -1,
     498,   114,   115,   116,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,    -1,
      13,    -1,    15,    -1,    -1,    -1,    -1,    -1,    -1,    22,
     538,    -1,   540,   541,   542,    -1,    -1,    -1,   546,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      43,    44,    45,    46,    47,    -1,    -1,    -1,   566,    52,
      53,    54,    55,    -1,    13,    58,    15,    60,    -1,    62,
      -1,   579,    -1,    22,    23,    -1,    -1,    -1,    -1,   587,
      -1,    -1,    -1,    -1,   592,    -1,    -1,   595,   596,    -1,
      -1,    -1,    -1,   601,    43,    44,    45,    46,    47,    -1,
      -1,   609,    -1,    52,    53,    54,    55,    -1,    -1,    -1,
      -1,    60,    -1,     1,    -1,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,   633,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    -1,   644,    -1,    -1,    27,
      -1,    29,    30,    31,    32,    33,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   667,
      -1,    -1,    50,    51,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    65,    66,    67,
      68,    69,    70,    71,    72,    73,    74,    75,    76,    77,
      78,    -1,   700,   701,    -1,   703,    84,    85,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   717,
     718,    -1,    -1,    -1,    -1,    -1,   724,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   114,   115,   116,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,     1,    -1,     3,     4,     5,    -1,
       7,     8,     9,    10,    -1,    13,    -1,    15,    -1,    -1,
      -1,    -1,    -1,    -1,    22,    -1,    23,    -1,    -1,    -1,
      27,    -1,    29,    30,    31,    32,    33,    -1,    35,    -1,
      -1,    -1,    -1,    -1,    -1,    43,    44,    45,    46,    47,
      -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,    56,
      -1,    58,    60,    -1,    61,    -1,    -1,    -1,    65,    66,
      67,    68,    69,    70,    71,    72,    73,    74,    75,    76,
      77,    78,    -1,    -1,    -1,    -1,    -1,    84,    85,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,   114,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,     1,    -1,     3,     4,     5,
      -1,     7,     8,     9,    10,    -1,    13,    -1,    15,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    22,    -1,    -1,    -1,
      -1,    27,    -1,    29,    30,    31,    32,    33,    -1,    35,
      -1,    -1,    -1,    -1,    -1,    -1,    43,    44,    45,    46,
      47,    -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,
      56,    -1,    58,    60,    -1,    61,    -1,    -1,    -1,    65,
      66,    67,    68,    69,    70,    71,    72,    73,    74,    75,
      76,    77,    78,    -1,    -1,    -1,    -1,    -1,    84,    85,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,   115,
     116,   117,   118,   119,   120,   121,   122,   123,   124,   125,
     126,   127,   128,   129,   130,   131,   132,   133,   134,   135,
     136,   137,   138,   139,   140,   141,     1,    -1,     3,     4,
       5,    -1,     7,     8,     9,    10,    13,    -1,    15,    -1,
      -1,    -1,    -1,    -1,    -1,    22,    -1,    -1,    -1,    -1,
      -1,    -1,    27,    -1,    29,    30,    31,    32,    33,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    43,    44,    45,    46,
      47,    -1,    -1,    -1,    -1,    52,    53,    54,    55,    -1,
      -1,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,
      65,    66,    67,    68,    69,    70,    71,    72,    73,    74,
      75,    76,    77,    78,    -1,    -1,    -1,    -1,    -1,    84,
      85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,     1,    -1,     3,
       4,     5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    27,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    13,    -1,    15,    -1,
      -1,    -1,    56,    57,    58,    22,    -1,    61,    -1,    -1,
      -1,    28,    66,    -1,    68,    69,    70,    71,    72,    73,
      74,    75,    76,    77,    78,    -1,    43,    44,    45,    46,
      47,    85,    -1,    -1,    -1,    52,    53,    54,    55,    -1,
      -1,    -1,    59,    60,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     114,   115,   116,   117,   118,   119,   120,   121,   122,   123,
     124,   125,   126,   127,   128,   129,   130,   131,   132,   133,
     134,   135,   136,   137,   138,   139,   140,   141,     1,    -1,
       3,     4,     5,    -1,     7,     8,     9,    10,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    27,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,
      -1,    -1,    -1,    66,    -1,    68,    69,    70,    71,    72,
      73,    74,    75,    76,    77,    78,    -1,    -1,    -1,    -1,
      -1,    -1,    85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,   114,   115,   116,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,     3,
       4,     5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    77,    78,    79,    -1,    -1,    -1,    -1,
      -1,    85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
     114,   115,   116,   117,   118,   119,   120,   121,   122,   123,
     124,   125,   126,   127,   128,   129,   130,   131,   132,   133,
     134,   135,   136,   137,   138,   139,   140,   141,     3,     4,
       5,    -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      35,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    77,    78,    -1,    -1,    -1,    -1,    -1,    -1,
      85,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,
     115,   116,   117,   118,   119,   120,   121,   122,   123,   124,
     125,   126,   127,   128,   129,   130,   131,   132,   133,   134,
     135,   136,   137,   138,   139,   140,   141,     3,     4,     5,
      -1,     7,     8,     9,    10,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      56,    -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    77,    78,    -1,    -1,    -1,    -1,    -1,    -1,    85,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,   115,
     116,   117,   118,   119,   120,   121,   122,   123,   124,   125,
     126,   127,   128,   129,   130,   131,   132,   133,   134,   135,
     136,   137,   138,   139,   140,   141,     3,     4,     5,    -1,
       7,     8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,
      -1,    58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      77,    78,    -1,    -1,    -1,    -1,    -1,    -1,    85,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,   114,   115,   116,
     117,   118,   119,   120,   121,   122,   123,   124,   125,   126,
     127,   128,   129,   130,   131,   132,   133,   134,   135,   136,
     137,   138,   139,   140,   141,     3,     4,     5,    -1,     7,
       8,     9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    35,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,
      58,    -1,    -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    77,
      78,    -1,    -1,    -1,    -1,    -1,    -1,    85,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,   114,   115,   116,   117,
     118,   119,   120,   121,   122,   123,   124,   125,   126,   127,
     128,   129,   130,   131,   132,   133,   134,   135,   136,   137,
     138,   139,   140,   141,     3,     4,     5,    -1,     7,     8,
       9,    10,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,     3,     4,     5,    35,     7,     8,     9,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    56,    -1,    58,
      -1,    -1,    61,     3,     4,     5,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    77,    78,
      -1,    -1,    -1,    -1,    -1,    -1,    85,    -1,    -1,    -1,
      -1,    61,    -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
      -1,    -1,    -1,    -1,    -1,    -1,    -1,    77,    78,    -1,
      -1,    -1,    -1,    -1,    -1,   114,   115,   116,   117,   118,
     119,   120,   121,   122,   123,   124,   125,   126,   127,   128,
     129,   130,   131,   132,   133,   134,   135,   136,   137,   138,
     139,   140,   141,    -1,   114,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,    -1,    -1,   114,   115,   116,   117,   118,   119,
     120,   121,   122,   123,   124,   125,   126,   127,   128,   129,
     130,   131,   132,   133,   134,   135,   136,   137,   138,   139,
     140,   141,    58,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
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
      26,    27,    66,   146,   147,   148,   149,   155,   156,   157,
     158,   159,   160,   232,    12,     1,     8,    15,    53,   150,
     151,   152,   153,   212,     9,   154,   212,   213,   215,   216,
     154,   154,   154,   154,   154,    18,     0,   232,    27,    66,
     147,     1,   150,    15,    53,    13,   152,    14,     1,    15,
      16,    17,    46,    47,    52,    53,    58,   196,    15,    15,
      22,   217,   196,   196,    22,    22,   164,   164,   154,   148,
     232,    13,    53,   214,   215,    14,   215,   212,     5,     6,
     212,   212,   212,   212,    10,    29,    35,    48,    56,    58,
      59,    68,    69,    70,    71,    72,    73,    74,    75,    76,
      85,   183,   197,   198,   200,   204,   215,   227,   229,    13,
     161,   162,     3,   220,   213,   213,   218,   219,   220,   164,
     164,   214,     1,     4,     5,     7,    29,    30,    31,    32,
      33,    35,    56,    58,    61,    65,    67,    77,    78,    84,
      85,   114,   115,   116,   117,   118,   119,   120,   121,   122,
     123,   124,   125,   126,   127,   128,   129,   130,   131,   132,
     133,   134,   135,   136,   137,   138,   139,   140,   141,   165,
     166,   167,   171,   177,   183,   185,   186,   187,   188,   189,
     190,   195,   199,   200,   204,   207,   212,   215,   220,   221,
     222,   223,   224,   225,   228,   229,   230,   196,   214,    28,
     215,    17,    17,    34,    15,   217,    56,   215,   183,   184,
     185,   215,   228,   183,   215,    15,   217,    58,    62,    28,
      59,    15,    34,    58,    62,   202,   213,   185,   215,   154,
     164,    62,   163,   217,    23,    28,    13,    23,   215,   215,
     215,   185,   215,    15,    22,    34,    36,   184,   183,   185,
     223,   230,    58,   183,   185,   210,   211,   228,   184,   212,
      22,    23,   232,    13,    15,    22,    43,    44,    45,    46,
      47,    52,    53,    54,    55,    60,    13,    15,    16,    22,
      43,    44,    45,    46,    47,    52,    53,    54,    55,    60,
     191,   192,   193,    58,    34,    58,   201,   202,    13,    46,
      47,    52,    53,    60,   179,   180,   183,   185,   204,    16,
     161,   215,   215,   220,    15,   184,    28,    57,    59,   220,
      15,    29,    35,    79,   183,   185,   205,   206,   215,   226,
     228,   203,   215,   198,   220,   215,   228,   205,    58,    80,
      81,    82,    83,    86,    87,    88,    89,    90,    91,    92,
      93,    94,    95,    96,    97,    98,    99,   100,   101,   102,
     103,   104,   105,   106,   107,   108,   109,   110,   111,   112,
     113,   231,    58,   214,   217,   219,   220,    22,   192,   192,
     164,   212,   218,    56,    56,    57,    28,    28,    59,   183,
     185,   211,    69,    37,    38,    39,    40,    41,    42,    48,
      49,    61,    50,    51,   164,   166,   209,   232,   196,    56,
     181,   182,   183,   213,   220,   166,   183,   185,   228,   212,
     182,   220,    49,    56,   164,   183,   200,   204,   183,   185,
     183,   185,   183,   185,   228,   183,   185,   228,   183,   185,
     228,   183,   185,   228,   183,   185,   228,   183,   185,   228,
     212,   215,   220,    29,    67,   194,   213,    56,   182,   183,
     185,   220,    49,   164,   183,   200,   204,   183,   185,   183,
     185,   183,   185,   228,   183,   185,   228,   183,   185,   228,
     183,   185,   228,   183,   185,   228,   193,   205,    56,   168,
     169,   183,   185,   228,   205,    28,    28,   183,   185,   228,
     183,   185,   228,   183,   185,   228,   183,   185,   228,   183,
     185,   228,   183,   185,   228,    28,   192,   192,   230,   164,
     220,    57,   183,   185,   228,    62,   220,    22,    28,    59,
      13,    34,    58,    15,    59,   231,    34,    52,    53,   205,
     165,   164,    34,    23,     1,   172,   173,   174,   185,   213,
     220,   229,   172,   184,   184,    59,    56,   183,   210,   210,
     210,   210,   210,   210,   210,   210,   210,   211,   211,    63,
      64,   208,   166,   164,   184,    28,    49,    58,    49,   178,
     183,   184,    58,   217,   217,    58,    58,   184,    49,    23,
     178,    58,   217,   217,    59,    35,   167,   170,   215,    28,
      59,   202,   202,   179,   183,   204,   203,   218,   206,    62,
     169,   169,   205,   220,   202,    59,   185,   228,   231,   231,
      59,    23,    56,    34,    57,   232,    34,   174,    57,    59,
      59,   184,   211,   209,    63,    64,    57,   181,   182,   205,
     182,    28,    49,    57,   205,   205,   168,    57,   182,    49,
     205,   217,    57,   232,   169,   201,   202,    58,    23,   203,
      59,   172,   168,   173,   175,   176,   183,   185,   220,   226,
      34,    57,   209,   211,   209,    23,    59,    23,   183,   164,
      59,    59,    59,    23,   164,    59,    35,   167,   215,   205,
      34,    58,    57,    28,    49,   191,   175,   209,   217,   217,
     217,    22,    59,   169,   205,   168,   176,    28,    28,   191,
     218,    59,   168,   168,    28,    23,   168
};
/* YYR1[RULE-NUM] -- Symbol kind of the left-hand side of rule RULE-NUM.  */
static const yytype_uint8 yyr1[] =
{
       0,   145,   146,   146,   146,   146,   146,   147,   147,   148,
     148,   148,   148,   148,   148,   148,   148,   149,   149,   149,
     149,   149,   149,   150,   150,   150,   150,   151,   151,   151,
     152,   152,   152,   152,   152,   152,   152,   153,   153,   153,
     153,   153,   153,   154,   154,   155,   155,   156,   157,   158,
     159,   160,   161,   162,   162,   163,   163,   164,   165,   165,
     165,   166,   166,   166,   166,   166,   166,   166,   166,   166,
     166,   166,   167,   167,   168,   168,   169,   169,   169,   169,
     170,   170,   171,   171,   171,   172,   172,   172,   173,   173,
     173,   173,   173,   173,   173,   173,   173,   174,   174,   174,
     175,   175,   176,   176,   176,   176,   177,   177,   177,   177,
     177,   178,   178,   179,   179,   180,   180,   181,   181,   181,
     182,   182,   183,   183,   183,   183,   183,   183,   183,   183,
     183,   183,   183,   183,   184,   184,   184,   184,   184,   184,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   185,
     185,   185,   185,   185,   185,   185,   185,   185,   185,   186,
     187,   188,   189,   190,   191,   191,   192,   192,   193,   193,
     194,   194,   194,   195,   196,   196,   196,   197,   197,   198,
     198,   198,   198,   198,   198,   198,   198,   198,   198,   198,
     198,   198,   198,   198,   199,   199,   200,   200,   201,   201,
     202,   202,   203,   203,   204,   204,   204,   205,   205,   205,
     206,   206,   206,   206,   206,   206,   206,   206,   207,   207,
     207,   207,   208,   208,   209,   209,   209,   210,   210,   210,
     211,   211,   211,   211,   211,   211,   211,   211,   211,   211,
     211,   211,   211,   212,   213,   213,   214,   214,   214,   215,
     216,   216,   217,   217,   217,   217,   218,   218,   219,   219,
     219,   220,   221,   222,   223,   223,   223,   224,   225,   225,
     226,   227,   228,   228,   228,   228,   228,   228,   228,   229,
     229,   229,   229,   229,   229,   229,   229,   229,   230,   230,
     230,   230,   230,   230,   230,   230,   230,   230,   230,   230,
     230,   230,   230,   230,   230,   230,   230,   230,   230,   230,
     230,   230,   230,   230,   230,   230,   231,   231,   231,   231,
     231,   231,   231,   231,   231,   231,   231,   231,   231,   231,
     231,   231,   231,   231,   231,   231,   231,   231,   231,   231,
     231,   231,   231,   231,   231,   231,   231,   231,   231,   231,
     231,   232,   232,   232,   232
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
       1,     1,     3,     6,     3,     1,     1,     1,     1,     3,
       3,     1,     5,     5,     7,     3,     1,     0,     3,     5,
       4,     6,     4,     5,     6,     7,     1,     1,     1,     1,
       3,     1,     1,     1,     1,     1,     3,     3,     1,     2,
       2,     3,     1,     1,     1,     3,     1,     1,     1,     3,
       3,     1,     1,     3,     1,     2,     3,     4,     1,     2,
       3,     4,     1,     3,     3,     3,     3,     1,     1,     1,
       1,     1,     2,     2,     2,     2,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       4,     6,     6,     6,     3,     3,     4,     6,     6,     6,
       6,     3,     3,     3,     3,     5,     7,     7,     7,     4,
       4,     4,     4,     3,     6,     3,     6,     5,     5,     5,
       3,     4,     2,     3,     1,     2,     0,     1,     2,     5,
       1,     1,     1,     4,     3,     2,     0,     3,     1,     1,
       3,     1,     3,     3,     2,     3,     4,     2,     2,     1,
       1,     3,     5,     5,     2,     5,     2,     5,     3,     3,
       1,     4,     1,     3,     6,     9,     8,     3,     1,     0,
       1,     1,     1,     1,     1,     3,     3,     6,     3,     5,
       4,     6,     3,     4,     1,     2,     1,     1,     1,     1,
       3,     3,     3,     3,     3,     3,     3,     3,     3,     3,
       3,     5,     3,     1,     1,     1,     3,     2,     1,     1,
       1,     2,     2,     3,     3,     4,     1,     3,     1,     1,
       3,     1,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     1,     1,     1,     1,     1,     3,     2,     2,     1,
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
       0,     0,     0,     0,     0,     0,     2,     1,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     1,     0,     2,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
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
       0,     0,     0,     0,     0,     0,     0,     0,    27,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    29,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     5,     0,    11,     0,     7,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,    13,     0,
       0,     0,     0,    17,    19,     0,     0,     0,     0,     0,
      21,     0,     0,     0,     0,     0,     0,     0,    45,     0,
      47,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    23,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    49,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,    33,     0,     0,     0,     0,     0,     0,    35,
       0,     0,     0,    37,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,    61,     0,     0,     0,     0,     0,     0,     0,    39,
      41,     0,    43,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    63,     9,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     1,     0,     0,     0,    15,     0,
       0,     3,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    25,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,    53,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,    51,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
      55,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,    57,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    59,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,    31,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
       0,     0,     0,     0,     0,     0,     0,     0,     0,     0,
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
       0,   329,     0,   329,     0,   122,     0,   122,     0,   325,
       0,   122,     0,   122,     0,   355,     0,   122,     0,   122,
       0,   122,     0,   122,     0,   280,     0,   122,     0,   122,
       0,   309,     0,   141,     0,   141,     0,   141,     0,   141,
       0,   141,     0,   141,     0,   122,     0,   122,     0,   122,
       0,   338,     0,   339,     0,   325,     0,   308,     0,   308,
       0,   122,     0,   122,     0
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
  "MCOP_PLUSMINUS", "MCPT_DBCOLON", "MCK_ELSE_IF", "MCK_ELSE", "MCK_IF",
  "MC_ENDL", "MCK_RETURN", "MCK_IO", "MCK_IN", "MCK_OUT", "MCK_ANL",
  "MCK_NC", "MCK_LABEL", "MCK_PSRC", "MCK_PSNK", "MCK_PSBI", "MCONST_HIGH",
  "MCONST_LOW", "MCONST_NC", "MCU_INT", "MCU_HEX", "MCU_FLOAT",
  "MCU_STRING", "MCK_FUNC", "MCK_THIS", "MCU_VOLT", "MCU_AMP", "MCU_CAP",
  "MCU_IND", "MCU_TIME", "MCU_LEN", "MCU_WATT", "MCU_OHM", "MCU_TEMP",
  "MCU_HZ", "MCU_DB", "MCU_PPM", "MCU_PERCENT", "MCU_BAUD", "MCU_DATASIZE",
  "MCU_SPS", "MCU_SIEMENS", "MCU_RESPONSIVITY", "MCU_ANGLE",
  "MCU_ANGULAR_RATE", "MCU_ENERGY", "MCU_EFIELD", "MCU_HFIELD", "MCU_FLUX",
  "MCU_BFIELD", "MCU_SLEW", "MCU_NOISE", "MCU_CHARGE", "MCUVAL_VOLT",
  "MCUVAL_AMP", "MCUVAL_CAP", "MCUVAL_IND", "MCUVAL_TIME", "MCUVAL_LEN",
  "MCUVAL_WATT", "MCUVAL_OHM", "MCUVAL_TEMP", "MCUVAL_HZ", "MCUVAL_DB",
  "MCUVAL_PPM", "MCUVAL_PERCENT", "MCUVAL_BAUD", "MCUVAL_DATASIZE",
  "MCUVAL_SPS", "MCUVAL_SIEMENS", "MCUVAL_RESPONSIVITY", "MCUVAL_ANGLE",
  "MCUVAL_ANGULAR_RATE", "MCUVAL_ENERGY", "MCUVAL_EFIELD", "MCUVAL_HFIELD",
  "MCUVAL_FLUX", "MCUVAL_BFIELD", "MCUVAL_SLEW", "MCUVAL_NOISE",
  "MCUVAL_CHARGE", "MC_WS", "MC_SINGLE_COMMENT", "MC_MULTI_COMMENT",
  "$accept", "start", "mc_tops", "mc_top", "mc_use", "mc_uri", "mc_prefix",
  "mc_uri_trunk", "mc_levels", "mc_class_name", "mc_component",
  "mc_module", "mc_interface", "mc_enum", "mc_define", "mc_capability",
  "mc_component_derivs", "mc_component_variant", "mc_component_adopts",
  "mc_body", "mc_clauses", "mc_clause", "mc_attribute", "mc_attr_values",
  "mc_attr_value", "mc_attr_lines", "mc_attribute_pin", "mc_pins_lines",
  "mc_pins_line", "mc_pin_idn", "mc_pins_names", "mc_pins_name", "mc_net",
  "mc_opds", "mc_net_port_elem", "mc_net_port_elems", "mc_mn_opd",
  "mc_mn_opds", "mc_opd", "mc_phrases", "mc_phrase", "mc_role",
  "mc_conduit", "mc_domain", "mc_rail", "mc_partition", "mc_tattrs",
  "mc_tattrs_opt", "mc_tattr", "mc_tkey", "mc_function", "mc_paramds",
  "mc_pards", "mc_pard", "mc_declare_a", "mc_declare_a1", "mc_insts",
  "mc_inst", "mc_iface_name", "mc_declare_b", "mc_params", "mc_param",
  "mc_conds", "mc_conds_elifs", "mc_cond_block", "mc_expr", "mc_judge",
  "mc_id", "mc_ida", "mc_idss", "mc_ids", "mc_idseg", "mc_idm", "mc_idans",
  "mc_idan", "mc_int", "mc_hex", "mc_float", "mc_number", "mc_string",
  "mc_const", "mc_nc", "mc_underscore", "mc_literal", "mc_iotype",
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
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COMPONENT, mc_value_link4(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 46:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COMPONENT, mc_value_link(
            mc_value_create_node(MCAST_ABSTRACT, NULL),
            mc_value_link4(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
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
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DEFINE, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 51:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_CAPABILITY, mc_value_link(mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
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
                 { ((*yyvalp).value) = NULL; mc_dlog_add(MCD_E1003_CLAUSE_SKIPPED, 1, g_last_token ? g_last_token->tpos : 0, g_last_token ? g_last_token->tlen : 0, NULL); }
    break;
  case 72:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 73:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
                    mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
                        mc_value_link(
                            mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                            mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
                    mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 74:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 75:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 76:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 77:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 78:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 79:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_SET_ATTRIBUTES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 80:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 81:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 82:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 83:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PINADD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 84:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) == NULL) {
        mc_dlog_add(MCD_W1106_EMPTY_PINS, 1,
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos + (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen, 0,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE_PIN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 85:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 86:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 87:
                                                    { ((*yyvalp).value) = NULL; }
    break;
  case 88:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 89:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 90:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link3(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 91:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 92:
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
  case 93:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_LINE, mc_value_link4(
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
                mc_value_create_node(MCAST_PIN_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PIN_NAMES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 94:
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
  case 95:
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
  case 96:
{
    ((*yyvalp).value) = NULL;
    mc_dlog_add(MCD_E1004_PIN_SKIPPED, 1,
                g_last_token ? g_last_token->tpos : 0,
                g_last_token ? g_last_token->tlen : 0,
                NULL);
}
    break;
  case 97:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 98:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 99:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 100:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 101:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 102:
{
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 103:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 104:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_PIN_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 105:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PIN_NAME, mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 106:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 107:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET_PORTS, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 108:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 109:
{
    
    if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 4 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 6 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 7 || (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type == 9)) {
        mc_dlog_add(MCD_E1007_NET_NOT_PORT, 1,
                    g_last_token->tpos, g_last_token->tlen,
                    NULL);
    }
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 110:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_NET, mc_value_link(
            mc_value_create_data(MCAST_IOTYPE_RETURN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 111:
                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 112:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 113:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 114:
                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 115:
                                                                 { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 116:
                                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 119:
          {
              
              ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
          }
    break;
  case 120:
                                             { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 121:
                                             { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 122:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 123:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 124:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD,
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 125:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 126:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 127:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_THIS, strdup("this"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 128:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD,
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 129:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 130:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 131:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, mc_value_link3(
            mc_value_create_data(MCAST_OPD_PINS, strdup("pins"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.token)->tlen),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 132:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 133:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 134:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 135:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 136:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 137:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 138:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 139:
                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 140:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 141:
                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 142:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 143:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1103_TRANSPOSE_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_APOST, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 144:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 145:
                                    { if ((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value) != NULL && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type >= 4 && (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)->type <= 9) { mc_dlog_add(MCD_W1104_CARET_ON_LITERAL, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen, NULL); } ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CARET, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 146:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 147:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 148:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 149:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 150:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 151:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 152:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 153:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 154:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_PLUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 155:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 156:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 157:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 158:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 159:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 160:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 161:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 162:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 163:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MINUS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 164:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 165:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 166:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 167:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 168:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 169:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 170:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 171:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 172:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_MULTI, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 173:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 174:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 175:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 176:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 177:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 178:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 179:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 180:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 181:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DIVID, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 182:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 183:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 184:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 185:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 186:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 187:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 188:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 189:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 190:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_TILDE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 191:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 192:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 193:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 194:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 195:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 196:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 197:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 198:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 199:
                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 200:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 201:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 202:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 203:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_RIGHTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 204:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 205:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 206:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 207:
                                        { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_LEFTARROW, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 208:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 209:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 210:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 211:
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
  case 212:
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
  case 213:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY_MN,
        mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_OPDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 214:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 215:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
}
    break;
  case 216:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 217:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 218:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 219:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 220:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 221:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 222:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 223:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 224:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
            );
}
    break;
  case 225:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 226:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 227:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 228:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link4(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 229:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 230:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 231:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 232:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_FCALL, mc_value_link3(
                mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
                (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
            );
}
    break;
  case 233:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 234:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 235:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 236:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CLOSURE, mc_value_link3(
            mc_value_create_node(MCAST_PARAMS_PRE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 237:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 238:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_GROUP, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 239:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ROLE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 240:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_REF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));   // MCAST_REF kept as internal node name
}
    break;
  case 241:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DOMAIN, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 242:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_RAIL, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 243:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARTITION, mc_value_link3(
                    mc_value_create_data(MCAST_PARTITION_LEVEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value),
                    (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 244:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 245:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 246:
{
    ((*yyvalp).value) = NULL;
}
    break;
  case 247:
{
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 248:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 249:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
            mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 250:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 251:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 252:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 253:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_FUNCTION, mc_value_link3(
            mc_value_create_node(MCAST_NAME, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
            mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 254:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 255:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 256:
                                                { ((*yyvalp).value) = NULL; }
    break;
  case 257:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 258:
                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 259:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 260:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link(
        mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.token)->tlen),
        (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 261:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 262:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 263:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 264:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_OPD, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 265:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 266:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)));
}
    break;
  case 267:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 268:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 269:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 270:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 271:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM,
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value))))
        );
}
    break;
  case 272:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM,
            mc_value_create_node(MCAST_DECLARE_UV, mc_value_link(
                mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
                mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))))
        );
}
    break;
  case 273:
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
  case 274:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 275:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 276: /* mc_declare_a1: mc_ids mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 277: /* mc_declare_a1: mc_ids MCPT_LPAREN mc_params MCPT_RPAREN mc_inst  */
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
            mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)))),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))
        );
}
    break;
  case 278:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 279:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 280:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 281:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value))));
}
    break;
  case 283:
                                        { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 284:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.value))));
}
    break;
  case 285:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                        mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                        mc_value_create_node(MCAST_INSTANCE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-8)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value))))
                    ));
}
    break;
  case 286:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_DECLARE, mc_value_link(
                mc_value_create_node(MCAST_CLASS, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_PARAMS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))),
                mc_value_create_node(MCAST_INSTANCE, mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-6)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 287:
                                            { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 288:
                                            { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 289:
                                            { ((*yyvalp).value) = NULL; }
    break;
  case 290:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 291:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 292:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_data(MCAST_ROLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen));
}
    break;
  case 293:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 294:
{
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
}
    break;
  case 295:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 296:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 297:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_PARAM, mc_value_create_node(MCAST_ATTRIBUTE, mc_value_link(
        mc_value_create_node(MCAST_ATT_ID, mc_value_create_node(MCAST_IDS,
            mc_value_link(
                mc_value_create_data(MCAST_OPD_PINS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-5)].yystate.yysemantics.yyval.token)->tlen),
                mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value))))),
        mc_value_create_node(MCAST_ATT_VALUES, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))));
}
    break;
  case 298:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_IF, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 299:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 300:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link3((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 301:
{
    
    ((*yyvalp).value) =  mc_value_create_node(MCAST_COND_IF, mc_value_link4(
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value),
            (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value),
            mc_value_create_node(MCAST_COND_ELSE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)))
        );
}
    break;
  case 302:
{
    
    ((*yyvalp).value) = mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
}
    break;
  case 303:
{
    
    ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-3)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_COND_ELSE, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))));
}
    break;
  case 304:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 305:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 306:
{
    
    ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value);
}
    break;
  case 307:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 308:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 309:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 310:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_EQEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 311:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_AND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 312:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_OR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 313:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_NOTEQ, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 314:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));}
    break;
  case 315:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATERTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 316:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_LESSEQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 317:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_GREATEREQTHAN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 318:
                                                { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITAND, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 319:
                                                { mc_dlog_add(MCD_W1101_SINGLE_OR, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_BITOR, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 320:
                                                { mc_dlog_add(MCD_W1102_PLUSMINUS, 2, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.token)->tlen, NULL); ((*yyvalp).value) = mc_value_create_node(MCAST_EXPRESSION, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 321:
                                                               { ((*yyvalp).value) = mc_value_create_node(MCAST_JUDGE_IN, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-4)].yystate.yysemantics.yyval.value), mc_value_create_node(MCAST_OPD_SQUARE_VEC, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)))); }
    break;
  case 322:
                                                { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 323:
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_ID, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 324:
                                                    { ((*yyvalp).value) = mc_value_create_data(MCAST_IDA, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 325:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 326:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 327:
                                                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value); }
    break;
  case 328:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 329:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_IDS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 330:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 331:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 332:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 333:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_DOT, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 334:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value)); }
    break;
  case 335:
                                                    { ((*yyvalp).value) = mc_value_link(mc_value_create_node(MCAST_OPD_CURLY, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value)), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 336:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 337:
                                                    { ((*yyvalp).value) = mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)); }
    break;
  case 338:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 339:
                                                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 340:
                                                    { ((*yyvalp).value) = mc_value_create_node(MCAST_OPD_COLON, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value))); }
    break;
  case 341:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 342:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 343:
                            { ((*yyvalp).value) = mc_value_create_data(MCAST_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 344:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 345:
                  { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 346:
                    { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 347:
                       { ((*yyvalp).value) = mc_value_create_data(MCAST_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 348:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 349:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_CONST, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 350:
                        { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 351:
                               { ((*yyvalp).value) = mc_value_create_data(MCAST_OPD_USCORE, strdup("_"), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 352:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 353:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 354:
                        { ((*yyvalp).value) = (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value); }
    break;
  case 355:
                        {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 356:
                                              {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_UVALUE_AT, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
        }
    break;
  case 357:
                                       {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 358:
                                   {
            
            ((*yyvalp).value) = mc_value_create_node(MCAST_RANGE_PLUSMINUS, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value));
        }
    break;
  case 359:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 360:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_OUT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 361:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_IO, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 362:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSRC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 363:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSNK, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 364:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_PSBI, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 365:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_ANL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 366:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_NC, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 367:
                        { ((*yyvalp).value) = mc_value_create_node(MCAST_IOTYPE, mc_value_create_data(MCAST_IOTYPE_LABEL, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen)); }
    break;
  case 368:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 369:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 370:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 371:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 372:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 373:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 374:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 375:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 376:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 377:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 378:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 379:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 380:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 381:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 382:
                  { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 383:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 384:
                 { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 385:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 386:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 387:
                      { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 388:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 389:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 390:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 391:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 392:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 393:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 394:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 395:
                { ((*yyvalp).value) = mc_value_create_data(MCAST_UVAL_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 396:
        { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_INT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 397:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HEX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 398:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLOAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 399:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_STRING, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 400:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_VOLT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 401:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_AMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 402:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CAP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 403:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_IND, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 404:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TIME, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 405:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_OHM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 406:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_TEMP, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 407:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HZ, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 408:
         { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DB, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 409:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PPM, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 410:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_LEN, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 411:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_WAT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 412:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_PERCENT, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 413:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BAUD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 414:
               { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_DATASIZE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 415:
          { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SPS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 416:
              { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SIEMENS, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 417:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_RESPONSIVITY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 418:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGLE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 419:
                   { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ANGULAR_RATE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 420:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_ENERGY, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 421:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_EFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 422:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_HFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 423:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_FLUX, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 424:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_BFIELD, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 425:
           { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_SLEW, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 426:
            { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_NOISE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 427:
             { ((*yyvalp).value) = mc_value_create_data(MCAST_UNIT_CHARGE, strdup((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tstring), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tpos, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.token)->tlen); }
    break;
  case 428:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_DIV, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 429:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_MUL, mc_value_link((YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-2)].yystate.yysemantics.yyval.value), (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (0)].yystate.yysemantics.yyval.value)));
	}
    break;
  case 430:
        {
	    
	    ((*yyvalp).value) = mc_value_create_node(MCAST_UNIT_GROUP, (YY_CAST (yyGLRStackItem const *, yyvsp)[YYFILL (-1)].yystate.yysemantics.yyval.value));
	}
    break;
  case 431:
                  {}
    break;
  case 432:
                         {}
    break;
  case 433:
                           {}
    break;
  case 434:
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
