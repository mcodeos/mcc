// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#ifndef _ASTDEF_H_
#define _ASTDEF_H_

#include "lex.h"
#include "common.h"

//0. basic type
#define MCAST_ID                       1
#define MCAST_IDA                      2
#define MCAST_IDS                      3
#define MCAST_INT                      4
#define MCAST_HEX                      5
#define MCAST_FLOAT                    6
#define MCAST_STRING                   7
#define MCAST_CONST                    8
#define MCAST_UVALUE                   9

//1. top
#define MCAST_COMPONENT                11
#define MCAST_MODULE                   12
#define MCAST_INTERFACE                13
#define MCAST_FUNCTION                 14
#define MCAST_ENUM                     15
#define MCAST_DEFINE                   16
#define MCAST_USE                      17
#define MCAST_USE_PUB                  18

//2. top-item
#define MCAST_NAME                     21
#define MCAST_PARAMS_PRE               22
#define MCAST_PARAMS                   23
#define MCAST_PARAM                    24
#define MCAST_BODY                     25
#define MCAST_DECLARE                  26
#define MCAST_DECLARE_UV               27
#define MCAST_CLASS                    28
#define MCAST_INSTANCE                 29

//3. clause
#define MCAST_ATTRIBUTE                30
#define MCAST_ATTRIBUTE_PIN            31
#define MCAST_ATTRIBUTE_PINADD         32
#define MCAST_NET                      33
#define MCAST_NET_PORTS                34
#define MCAST_EXPRESSION               35
#define MCAST_ROLE                     36
#define MCAST_ENUM_VALUES              37

#define MCAST_IOTYPE                   38
#define MCAST_IOTYPE_IN                39
#define MCAST_IOTYPE_OUT               40
#define MCAST_IOTYPE_IO                41
#define MCAST_IOTYPE_RETURN            42
#define MCAST_IOTYPE_PS                43
#define MCAST_IOTYPE_ANL               44
#define MCAST_IOTYPE_NC                45

//3.1 attr
#define MCAST_ATT_ID                   46
#define MCAST_ATT_VALUES               47

//3.2 pin
#define MCAST_PIN_LINE                 48
#define MCAST_PIN_ID                   49
#define MCAST_PIN_NAMES                50
#define MCAST_PIN_NAME                 51
#define MCAST_PIN_VALUES               MCAST_ATT_VALUES //47

//3.3 net
// element
#define MCAST_OPD                       52
#define MCAST_OPD_THIS                  53
#define MCAST_OPD_PINS                  54
#define MCAST_OPD_MEMBER                55
#define MCAST_OPD_IDAN                  56
#define MCAST_OPD_USCORE                57
#define MCAST_OPD_NC                    58

// transform
#define MCAST_OPD_DOT                   59
#define MCAST_OPD_CURLY                 60
#define MCAST_OPD_CURLY_MN              61
#define MCAST_OPD_SQUARE_VEC            62
#define MCAST_OPR_PAREN                 63
#define MCAST_OPD_APOST                 64
#define MCAST_OPD_CARET                 65

// operator
#define MCAST_OPD_PLUS                  66
#define MCAST_OPD_MINUS                 67
#define MCAST_OPD_MULTI                 68
#define MCAST_OPD_DIVID                 69
#define MCAST_OPD_LEFTARROW             70
#define MCAST_OPD_RIGHTARROW            71

// composition
#define MCAST_OPD_FCALL                 72
#define MCAST_OPD_CLOSURE               73
#define MCAST_OPD_GROUP                 74
#define MCAST_OPD_TILDE                 75
#define MCAST_OPD_COLON                 76
#define MCAST_OPD_DBCOLON               77

//3.4 
#define MCAST_COND_IF                   81
#define MCAST_COND_ELSE                 82
#define MCAST_COND_BLOCK                83
#define MCAST_JUDGE_EQEQ                84
#define MCAST_JUDGE_NOTEQ               85
#define MCAST_JUDGE_LESSTHAN            86
#define MCAST_JUDGE_GREATERTHAN         87
#define MCAST_JUDGE_LESSEQTHAN          88
#define MCAST_JUDGE_GREATEREQTHAN       89
#define MCAST_JUDGE_BITAND              90
#define MCAST_JUDGE_BITOR               91
#define MCAST_JUDGE_IN                  92

//4. use
#define MCAST_URI_PREFIX                101
#define MCAST_URI_VERSION               102
#define MCAST_URI_MODULE                103
#define MCAST_URI_FILE                  104
#define MCAST_URI_ASID                  105
#define MCAST_URI_IMPORT_IDS            106

//5. support data
#define MCAST_SET                       111
#define MCAST_SET_ATTRIBUTES            112
#define MCAST_KVS                       113
#define MCAST_KVS_KEY                   114
#define MCAST_KVS_VALUE                 115
#define MCAST_RANGE_PLUSMINUS           117
#define MCAST_UVALUE_AT                 118
#define MCAST_SQUARE_VEC                119

//6. unit value
#define MCAST_UVAL_VOLT                 201
#define MCAST_UVAL_AMP                  202
#define MCAST_UVAL_CAP                  203
#define MCAST_UVAL_IND                  204
#define MCAST_UVAL_TIME                 205
#define MCAST_UVAL_LEN                  206
#define MCAST_UVAL_WAT                  207
#define MCAST_UVAL_OHM                  208
#define MCAST_UVAL_TEMP                 209
#define MCAST_UVAL_HZ                   210
#define MCAST_UVAL_DB                   211
#define MCAST_UVAL_PPM                  212
#define MCAST_UVAL_PERCENT              213
#define MCAST_UVAL_BAUD                 214
#define MCAST_UVAL_DATASIZE             215
#define MCAST_UVAL_SPS                  216
#define MCAST_UVAL_SIEMENS              217
#define MCAST_UVAL_RESPONSIVITY         218
#define MCAST_UVAL_ANGLE                219
#define MCAST_UVAL_ANGULAR_RATE         220
#define MCAST_UVAL_ENERGY               221
#define MCAST_UVAL_EFIELD               222
#define MCAST_UVAL_HFIELD               223
#define MCAST_UVAL_FLUX                 224
#define MCAST_UVAL_BFIELD               225
#define MCAST_UVAL_SLEW                 226
#define MCAST_UVAL_NOISE                227

//. units
#define MCAST_UNIT_INT                  301
#define MCAST_UNIT_HEX                  302
#define MCAST_UNIT_FLOAT                303
#define MCAST_UNIT_STRING               304

#define MCAST_UNIT_VOLT                 305
#define MCAST_UNIT_AMP                  306
#define MCAST_UNIT_CAP                  307
#define MCAST_UNIT_IND                  308
#define MCAST_UNIT_TIME                 309
#define MCAST_UNIT_LEN                  310
#define MCAST_UNIT_WAT                  311
#define MCAST_UNIT_OHM                  312
#define MCAST_UNIT_TEMP                 313
#define MCAST_UNIT_HZ                   314
#define MCAST_UNIT_DB                   315
#define MCAST_UNIT_PPM                  316
#define MCAST_UNIT_PERCENT              317
#define MCAST_UNIT_BAUD                 318
#define MCAST_UNIT_DATASIZE             319
#define MCAST_UNIT_SPS                  320
#define MCAST_UNIT_SIEMENS              321
#define MCAST_UNIT_RESPONSIVITY         322
#define MCAST_UNIT_ANGLE                323
#define MCAST_UNIT_ANGULAR_RATE         324
#define MCAST_UNIT_ENERGY               325
#define MCAST_UNIT_EFIELD               326
#define MCAST_UNIT_HFIELD               327
#define MCAST_UNIT_FLUX                 328
#define MCAST_UNIT_BFIELD               329
#define MCAST_UNIT_SLEW                 330
#define MCAST_UNIT_NOISE                331


//---------------------------
typedef struct mc_value {
    unsigned short type;
    void* data;
    unsigned int pos;
    unsigned int len;
    struct mc_value* next;
    struct mc_value* sub;
} mc_value;

mc_value* mc_value_create(void);
mc_value* mc_value_create_node(unsigned short type, mc_value* sub);
mc_value* mc_value_create_data(unsigned short type, void* data, unsigned int pos, unsigned int len);
mc_value* mc_value_link(mc_value* va, mc_value* vb);
mc_value* mc_value_link3(mc_value* va, mc_value* vb, mc_value* vc);
mc_value* mc_value_link4(mc_value* va, mc_value* vb, mc_value* vc, mc_value* vd);

//---------------------------
void mcc_reset(unsigned char log_flags);
void mcc_lex(char* data);
mc_value* mcc_parse();
void mcc_free(mc_value* ast);
void mcc_visit(mc_value* ast);

// Bison parser function
extern int mca_parse(mc_value* mcast);

// Lexer global variables
extern mc_lex_token* g_current_token;

// mc_value free function
extern void mc_value_free(mc_value** value);

//---------------------------
// #define DEBUG_TOKEN

typedef struct mc_sem_token {
    short type;
    #ifdef DEBUG_TOKEN
    char* string;
    #endif
    unsigned int pos;
    unsigned int len;
    struct mc_sem_token* next;
} mc_sem_token;

mc_sem_token* mcc_get_sem_tokens();
void mc_sem_token_add(short type, void* token);
void mc_sem_token_add_mcvalue(short type, mc_value* pval);
void mc_sem_token_print();
void mc_sem_token_free();

mc_sem_token* mcc_get_error_tokens();
void mc_error_token_add(void* ptoken);
void mc_error_token_print();
void mc_error_token_free();

#define MCC_TK_STRING       0
#define MCC_TK_NUMBER       1
#define MCC_TK_TYPE         2
#define MCC_TK_CLASS        3
#define MCC_TK_FUNCTION     4
#define MCC_TK_INTERFACE    5
#define MCC_TK_ENUM         6
#define MCC_TK_TYPEPARAM    7
#define MCC_TK_PARAM        8
#define MCC_TK_VARIABLE     9
#define MCC_TK_PROPERTY     10
#define MCC_TK_METHOD       11
#define MCC_TK_MACRO        12
#define MCC_TK_KEYWORD      13
#define MCC_TK_OPERATOR     14
#define MCC_TK_REGEXP       15
#define MCC_TK_COMMENT      16
#define MCC_TK_COMMENT_ML   101
#define MCC_TK_NONE         255


////////////////////////////////////////////////////////////////////////////
// #define NDEBUG
void* ParseAlloc(void* (*mallocProc)(size_t));
void Parse(void* yyp, int yymajor, mc_lex_token* yyminor, mc_value* mcast);
void ParseFree(void* p, void (*freeProc)(void*));
#ifndef NDEBUG
void ParseTrace(FILE *TraceFILE, char *zTracePrompt);
#endif

#endif
