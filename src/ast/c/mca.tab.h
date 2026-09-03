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
    MCK_CAPABILITY = 280,          /* MCK_CAPABILITY  */
    MCK_ABSTRACT = 281,            /* MCK_ABSTRACT  */
    MCPT_SEMICOLON = 282,          /* MCPT_SEMICOLON  */
    MCPT_COMMA = 283,              /* MCPT_COMMA  */
    MCK_ROLE = 284,                /* MCK_ROLE  */
    MCOP_EQUAL = 285,              /* MCOP_EQUAL  */
    MCK_PINS = 286,                /* MCK_PINS  */
    MCOP_PLUSEQUAL = 287,          /* MCOP_PLUSEQUAL  */
    MCOP_EQUALEQUAL = 288,         /* MCOP_EQUALEQUAL  */
    MCOP_NOTEQUAL = 289,           /* MCOP_NOTEQUAL  */
    MCOP_LESSTHAN = 290,           /* MCOP_LESSTHAN  */
    MCOP_GREATERTHAN = 291,        /* MCOP_GREATERTHAN  */
    MCOP_LESSEQTHAN = 292,         /* MCOP_LESSEQTHAN  */
    MCOP_GREATEREQTHAN = 293,      /* MCOP_GREATEREQTHAN  */
    MCOP_DOUBLEARROW = 294,        /* MCOP_DOUBLEARROW  */
    MCOP_LEFTARROW = 295,          /* MCOP_LEFTARROW  */
    MCOP_RIGHTARROW = 296,         /* MCOP_RIGHTARROW  */
    MCOP_PLUS = 297,               /* MCOP_PLUS  */
    MCOP_MINUS = 298,              /* MCOP_MINUS  */
    MCOP_AND = 299,                /* MCOP_AND  */
    MCOP_OR = 300,                 /* MCOP_OR  */
    MCOP_MULTI = 301,              /* MCOP_MULTI  */
    MCOP_DIVID = 302,              /* MCOP_DIVID  */
    MCOP_CARET = 303,              /* MCOP_CARET  */
    MCOP_APOST = 304,              /* MCOP_APOST  */
    MCPT_LBRACKET = 305,           /* MCPT_LBRACKET  */
    MCPT_RBRACKET = 306,           /* MCPT_RBRACKET  */
    MCPT_LPAREN = 307,             /* MCPT_LPAREN  */
    MCPT_RPAREN = 308,             /* MCPT_RPAREN  */
    MCOP_TILDE = 309,              /* MCOP_TILDE  */
    MCOP_PLUSMINUS = 310,          /* MCOP_PLUSMINUS  */
    MCPT_DBCOLON = 311,            /* MCPT_DBCOLON  */
    MCK_ELSE_IF = 312,             /* MCK_ELSE_IF  */
    MCK_ELSE = 313,                /* MCK_ELSE  */
    MCK_IF = 314,                  /* MCK_IF  */
    MC_ENDL = 315,                 /* MC_ENDL  */
    MCK_RETURN = 316,              /* MCK_RETURN  */
    MCK_IO = 317,                  /* MCK_IO  */
    MCK_IN = 318,                  /* MCK_IN  */
    MCK_OUT = 319,                 /* MCK_OUT  */
    MCK_PS = 320,                  /* MCK_PS  */
    MCK_ANL = 321,                 /* MCK_ANL  */
    MCK_NC = 322,                  /* MCK_NC  */
    MCK_LABEL = 323,               /* MCK_LABEL  */
    MCONST_HIGH = 324,             /* MCONST_HIGH  */
    MCONST_LOW = 325,              /* MCONST_LOW  */
    MCONST_NC = 326,               /* MCONST_NC  */
    MCU_INT = 327,                 /* MCU_INT  */
    MCU_HEX = 328,                 /* MCU_HEX  */
    MCU_FLOAT = 329,               /* MCU_FLOAT  */
    MCU_STRING = 330,              /* MCU_STRING  */
    MCK_FUNC = 331,                /* MCK_FUNC  */
    MCK_THIS = 332,                /* MCK_THIS  */
    MCU_VOLT = 333,                /* MCU_VOLT  */
    MCU_AMP = 334,                 /* MCU_AMP  */
    MCU_CAP = 335,                 /* MCU_CAP  */
    MCU_IND = 336,                 /* MCU_IND  */
    MCU_TIME = 337,                /* MCU_TIME  */
    MCU_LEN = 338,                 /* MCU_LEN  */
    MCU_WATT = 339,                /* MCU_WATT  */
    MCU_OHM = 340,                 /* MCU_OHM  */
    MCU_TEMP = 341,                /* MCU_TEMP  */
    MCU_HZ = 342,                  /* MCU_HZ  */
    MCU_DB = 343,                  /* MCU_DB  */
    MCU_PPM = 344,                 /* MCU_PPM  */
    MCU_PERCENT = 345,             /* MCU_PERCENT  */
    MCU_BAUD = 346,                /* MCU_BAUD  */
    MCU_DATASIZE = 347,            /* MCU_DATASIZE  */
    MCU_SPS = 348,                 /* MCU_SPS  */
    MCU_SIEMENS = 349,             /* MCU_SIEMENS  */
    MCU_RESPONSIVITY = 350,        /* MCU_RESPONSIVITY  */
    MCU_ANGLE = 351,               /* MCU_ANGLE  */
    MCU_ANGULAR_RATE = 352,        /* MCU_ANGULAR_RATE  */
    MCU_ENERGY = 353,              /* MCU_ENERGY  */
    MCU_EFIELD = 354,              /* MCU_EFIELD  */
    MCU_HFIELD = 355,              /* MCU_HFIELD  */
    MCU_FLUX = 356,                /* MCU_FLUX  */
    MCU_BFIELD = 357,              /* MCU_BFIELD  */
    MCU_SLEW = 358,                /* MCU_SLEW  */
    MCU_NOISE = 359,               /* MCU_NOISE  */
    MCU_CHARGE = 360,              /* MCU_CHARGE  */
    MCUVAL_VOLT = 361,             /* MCUVAL_VOLT  */
    MCUVAL_AMP = 362,              /* MCUVAL_AMP  */
    MCUVAL_CAP = 363,              /* MCUVAL_CAP  */
    MCUVAL_IND = 364,              /* MCUVAL_IND  */
    MCUVAL_TIME = 365,             /* MCUVAL_TIME  */
    MCUVAL_LEN = 366,              /* MCUVAL_LEN  */
    MCUVAL_WATT = 367,             /* MCUVAL_WATT  */
    MCUVAL_OHM = 368,              /* MCUVAL_OHM  */
    MCUVAL_TEMP = 369,             /* MCUVAL_TEMP  */
    MCUVAL_HZ = 370,               /* MCUVAL_HZ  */
    MCUVAL_DB = 371,               /* MCUVAL_DB  */
    MCUVAL_PPM = 372,              /* MCUVAL_PPM  */
    MCUVAL_PERCENT = 373,          /* MCUVAL_PERCENT  */
    MCUVAL_BAUD = 374,             /* MCUVAL_BAUD  */
    MCUVAL_DATASIZE = 375,         /* MCUVAL_DATASIZE  */
    MCUVAL_SPS = 376,              /* MCUVAL_SPS  */
    MCUVAL_SIEMENS = 377,          /* MCUVAL_SIEMENS  */
    MCUVAL_RESPONSIVITY = 378,     /* MCUVAL_RESPONSIVITY  */
    MCUVAL_ANGLE = 379,            /* MCUVAL_ANGLE  */
    MCUVAL_ANGULAR_RATE = 380,     /* MCUVAL_ANGULAR_RATE  */
    MCUVAL_ENERGY = 381,           /* MCUVAL_ENERGY  */
    MCUVAL_EFIELD = 382,           /* MCUVAL_EFIELD  */
    MCUVAL_HFIELD = 383,           /* MCUVAL_HFIELD  */
    MCUVAL_FLUX = 384,             /* MCUVAL_FLUX  */
    MCUVAL_BFIELD = 385,           /* MCUVAL_BFIELD  */
    MCUVAL_SLEW = 386,             /* MCUVAL_SLEW  */
    MCUVAL_NOISE = 387,            /* MCUVAL_NOISE  */
    MCUVAL_CHARGE = 388,           /* MCUVAL_CHARGE  */
    MC_WS = 389,                   /* MC_WS  */
    MC_SINGLE_COMMENT = 390,       /* MC_SINGLE_COMMENT  */
    MC_MULTI_COMMENT = 391         /* MC_MULTI_COMMENT  */
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
