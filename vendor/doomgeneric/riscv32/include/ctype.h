/* Minimal freestanding ctype for the Setun-Doom RV32IM port. */
#ifndef SETUN_CTYPE_H
#define SETUN_CTYPE_H

int isspace(int c);
int isdigit(int c);
int isxdigit(int c);
int isalpha(int c);
int isalnum(int c);
int isprint(int c);
int isgraph(int c);
int ispunct(int c);
int iscntrl(int c);
int isupper(int c);
int islower(int c);
int toupper(int c);
int tolower(int c);

#endif
