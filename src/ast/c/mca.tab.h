/* A Bison parser, made by GNU Bison 3.8.2.  */

/* Skeleton interface for Bison GLR parsers in C

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

#ifndef YY_MCA_MCA_TAB_H_INCLUDED
# define YY_MCA_MCA_TAB_H_INCLUDED
/* Debug traces.  */
#ifndef MCA_DEBUG
# if defined YYDEBUG
#if YYDEBUG
#   define MCA_DEBUG 1
#  else
#   define MCA_DEBUG 0
#  endif
# else /* ! defined YYDEBUG */
#  define MCA_DEBUG 1
# endif /* ! defined YYDEBUG */
#endif  /* ! defined MCA_DEBUG */
#if MCA_DEBUG
extern int mca_debug;
#endif

/* Token kinds.  */
#ifndef MCA_TOKENTYPE
# define MCA_TOKENTYPE
  enum mca_tokentype
  {
    MCA_EMPTY = -2,
    MCA_EOF = 0,                   /* "end of file"  */
    MCA_error = 256,               /* error  */
    MCA_UNDEF = 257,               /* "invalid token"  */
    MCTP_NUMBER_DEC = 258,         /* MCTP_NUMBER_DEC  */
    MCTP_NUMBER_HEX = 259,         /* MCTP_NUMBER_HEX  */
    MCTP_NUMBER_FLOAT = 260,       /* MCTP_NUMBER_FLOAT  */
    MCTP_VERSION = 261,            /* MCTP_VERSION  */
    MCTP_STRING = 262,             /* MCTP_STRING  */
    MCTP_ID = 263,                 /* MCTP_ID  */
    MCTP_IDA = 264,                /* MCTP_IDA  */
    MCOP_UNDERSCORE = 265,         /* MCOP_UNDERSCORE  */
    MCK_PUB = 266,                 /* MCK_PUB  */
    MCK_USE = 267,                 /* MCK_USE  */
    MCPT_COLON = 268,              /* MCPT_COLON  */
    MCK_AS = 269,                  /* MCK_AS  */
    MCPT_DOT = 270,                /* MCPT_DOT  */
    MCPT_AT = 271,                 /* MCPT_AT  */
    MCK_MC = 272,                  /* MCK_MC  */
    MCK_COMPONENT = 273,           /* MCK_COMPONENT  */
    MCK_MODULE = 274,              /* MCK_MODULE  */
    MCK_INTERFACE = 275,           /* MCK_INTERFACE  */
    MCK_ENUM = 276,                /* MCK_ENUM  */
    MCPT_LCURLY = 277,             /* MCPT_LCURLY  */
    MCPT_RCURLY = 278,             /* MCPT_RCURLY  */
    MCK_DEFINE = 279,              /* MCK_DEFINE  */
    MCPT_SEMICOLON = 280,          /* MCPT_SEMICOLON  */
    MCPT_COMMA = 281,              /* MCPT_COMMA  */
    MCK_ROLE = 282,                /* MCK_ROLE  */
    MCOP_EQUAL = 283,              /* MCOP_EQUAL  */
    MCK_PINS = 284,                /* MCK_PINS  */
    MCOP_PLUSEQUAL = 285,          /* MCOP_PLUSEQUAL  */
    MCOP_EQUALEQUAL = 286,         /* MCOP_EQUALEQUAL  */
    MCOP_NOTEQUAL = 287,           /* MCOP_NOTEQUAL  */
    MCOP_LESSTHAN = 288,           /* MCOP_LESSTHAN  */
    MCOP_GREATERTHAN = 289,        /* MCOP_GREATERTHAN  */
    MCOP_LESSEQTHAN = 290,         /* MCOP_LESSEQTHAN  */
    MCOP_GREATEREQTHAN = 291,      /* MCOP_GREATEREQTHAN  */
    MCOP_DOUBLEARROW = 292,        /* MCOP_DOUBLEARROW  */
    MCOP_LEFTARROW = 293,          /* MCOP_LEFTARROW  */
    MCOP_RIGHTARROW = 294,         /* MCOP_RIGHTARROW  */
    MCOP_PLUS = 295,               /* MCOP_PLUS  */
    MCOP_MINUS = 296,              /* MCOP_MINUS  */
    MCOP_AND = 297,                /* MCOP_AND  */
    MCOP_OR = 298,                 /* MCOP_OR  */
    MCOP_MULTI = 299,              /* MCOP_MULTI  */
    MCOP_DIVID = 300,              /* MCOP_DIVID  */
    MCOP_CARET = 301,              /* MCOP_CARET  */
    MCOP_APOST = 302,              /* MCOP_APOST  */
    MCPT_LBRACKET = 303,           /* MCPT_LBRACKET  */
    MCPT_RBRACKET = 304,           /* MCPT_RBRACKET  */
    MCPT_LPAREN = 305,             /* MCPT_LPAREN  */
    MCPT_RPAREN = 306,             /* MCPT_RPAREN  */
    MCOP_TILDE = 307,              /* MCOP_TILDE  */
    MCOP_PLUSMINUS = 308,          /* MCOP_PLUSMINUS  */
    MCPT_DBCOLON = 309,            /* MCPT_DBCOLON  */
    MCK_ELSE_IF = 310,             /* MCK_ELSE_IF  */
    MCK_ELSE = 311,                /* MCK_ELSE  */
    MCK_IF = 312,                  /* MCK_IF  */
    MC_ENDL = 313,                 /* MC_ENDL  */
    MCK_RETURN = 314,              /* MCK_RETURN  */
    MCK_IO = 315,                  /* MCK_IO  */
    MCK_IN = 316,                  /* MCK_IN  */
    MCK_OUT = 317,                 /* MCK_OUT  */
    MCK_PS = 318,                  /* MCK_PS  */
    MCK_ANL = 319,                 /* MCK_ANL  */
    MCK_NC = 320,                  /* MCK_NC  */
    MCK_LABEL = 321,               /* MCK_LABEL  */
    MCONST_HIGH = 322,             /* MCONST_HIGH  */
    MCONST_LOW = 323,              /* MCONST_LOW  */
    MCONST_NC = 324,               /* MCONST_NC  */
    MCU_INT = 325,                 /* MCU_INT  */
    MCU_HEX = 326,                 /* MCU_HEX  */
    MCU_FLOAT = 327,               /* MCU_FLOAT  */
    MCU_STRING = 328,              /* MCU_STRING  */
    MCK_FUNC = 329,                /* MCK_FUNC  */
    MCK_THIS = 330,                /* MCK_THIS  */
    MCU_VOLT = 331,                /* MCU_VOLT  */
    MCU_AMP = 332,                 /* MCU_AMP  */
    MCU_CAP = 333,                 /* MCU_CAP  */
    MCU_IND = 334,                 /* MCU_IND  */
    MCU_TIME = 335,                /* MCU_TIME  */
    MCU_LEN = 336,                 /* MCU_LEN  */
    MCU_WAT = 337,                 /* MCU_WAT  */
    MCU_OHM = 338,                 /* MCU_OHM  */
    MCU_TEMP = 339,                /* MCU_TEMP  */
    MCU_HZ = 340,                  /* MCU_HZ  */
    MCU_DB = 341,                  /* MCU_DB  */
    MCU_PPM = 342,                 /* MCU_PPM  */
    MCU_PERCENT = 343,             /* MCU_PERCENT  */
    MCU_BAUD = 344,                /* MCU_BAUD  */
    MCU_DATASIZE = 345,            /* MCU_DATASIZE  */
    MCU_SPS = 346,                 /* MCU_SPS  */
    MCU_SIEMENS = 347,             /* MCU_SIEMENS  */
    MCU_RESPONSIVITY = 348,        /* MCU_RESPONSIVITY  */
    MCU_ANGLE = 349,               /* MCU_ANGLE  */
    MCU_ANGULAR_RATE = 350,        /* MCU_ANGULAR_RATE  */
    MCU_ENERGY = 351,              /* MCU_ENERGY  */
    MCU_EFIELD = 352,              /* MCU_EFIELD  */
    MCU_HFIELD = 353,              /* MCU_HFIELD  */
    MCU_FLUX = 354,                /* MCU_FLUX  */
    MCU_BFIELD = 355,              /* MCU_BFIELD  */
    MCU_SLEW = 356,                /* MCU_SLEW  */
    MCU_NOISE = 357,               /* MCU_NOISE  */
    MCUVAL_VOLT = 358,             /* MCUVAL_VOLT  */
    MCUVAL_AMP = 359,              /* MCUVAL_AMP  */
    MCUVAL_CAP = 360,              /* MCUVAL_CAP  */
    MCUVAL_IND = 361,              /* MCUVAL_IND  */
    MCUVAL_TIME = 362,             /* MCUVAL_TIME  */
    MCUVAL_LEN = 363,              /* MCUVAL_LEN  */
    MCUVAL_WAT = 364,              /* MCUVAL_WAT  */
    MCUVAL_OHM = 365,              /* MCUVAL_OHM  */
    MCUVAL_TEMP = 366,             /* MCUVAL_TEMP  */
    MCUVAL_HZ = 367,               /* MCUVAL_HZ  */
    MCUVAL_DB = 368,               /* MCUVAL_DB  */
    MCUVAL_PPM = 369,              /* MCUVAL_PPM  */
    MCUVAL_PERCENT = 370,          /* MCUVAL_PERCENT  */
    MCUVAL_BAUD = 371,             /* MCUVAL_BAUD  */
    MCUVAL_DATASIZE = 372,         /* MCUVAL_DATASIZE  */
    MCUVAL_SPS = 373,              /* MCUVAL_SPS  */
    MCUVAL_SIEMENS = 374,          /* MCUVAL_SIEMENS  */
    MCUVAL_RESPONSIVITY = 375,     /* MCUVAL_RESPONSIVITY  */
    MCUVAL_ANGLE = 376,            /* MCUVAL_ANGLE  */
    MCUVAL_ANGULAR_RATE = 377,     /* MCUVAL_ANGULAR_RATE  */
    MCUVAL_ENERGY = 378,           /* MCUVAL_ENERGY  */
    MCUVAL_EFIELD = 379,           /* MCUVAL_EFIELD  */
    MCUVAL_HFIELD = 380,           /* MCUVAL_HFIELD  */
    MCUVAL_FLUX = 381,             /* MCUVAL_FLUX  */
    MCUVAL_BFIELD = 382,           /* MCUVAL_BFIELD  */
    MCUVAL_SLEW = 383,             /* MCUVAL_SLEW  */
    MCUVAL_NOISE = 384,            /* MCUVAL_NOISE  */
    MC_WS = 385,                   /* MC_WS  */
    MC_SINGLE_COMMENT = 386,       /* MC_SINGLE_COMMENT  */
    MC_MULTI_COMMENT = 387         /* MC_MULTI_COMMENT  */
  };
  typedef enum mca_tokentype mca_token_kind_t;
#endif

/* Value type.  */
#if ! defined MCA_STYPE && ! defined MCA_STYPE_IS_DECLARED
union MCA_STYPE
{

    mc_lex_token *token;
    mc_value *value;


};
typedef union MCA_STYPE MCA_STYPE;
# define MCA_STYPE_IS_TRIVIAL 1
# define MCA_STYPE_IS_DECLARED 1
#endif

/* Location type.  */
#if ! defined MCA_LTYPE && ! defined MCA_LTYPE_IS_DECLARED
typedef struct MCA_LTYPE MCA_LTYPE;
struct MCA_LTYPE
{
  int first_line;
  int first_column;
  int last_line;
  int last_column;
};
# define MCA_LTYPE_IS_DECLARED 1
# define MCA_LTYPE_IS_TRIVIAL 1
#endif



int mca_parse (mc_value* mcast);

#endif /* !YY_MCA_MCA_TAB_H_INCLUDED  */
