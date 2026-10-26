/*
 * minilibc.h — Freestanding C runtime header for Setun-Doom
 * Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
 * SPDX-License-Identifier: MIT
 */

#ifndef SETUN_MINILIBC_H
#define SETUN_MINILIBC_H

typedef unsigned long size_t;
#define NULL ((void *)0)

/* Memory manipulation */
void *memcpy(void *dest, const void *src, size_t n);
void *memset(void *s, int c, size_t n);
void *memmove(void *dest, const void *src, size_t n);
int memcmp(const void *s1, const void *s2, size_t n);

/* String routines */
size_t strlen(const char *s);
char *strcpy(char *dest, const char *src);
char *strncpy(char *dest, const char *src, size_t n);
char *strcat(char *dest, const char *src);
int strcmp(const char *s1, const char *s2);
int strncmp(const char *s1, const char *s2, size_t n);
char *strchr(const char *s, int c);
char *strrchr(const char *s, int c);
char *strstr(const char *haystack, const char *needle);

/* Character & parsing */
int toupper(int c);
int tolower(int c);
int isspace(int c);
int isdigit(int c);
int atoi(const char *nptr);
int abs(int j);

/* Dynamic allocation */
void *malloc(size_t size);
void free(void *ptr);
void *realloc(void *ptr, size_t size);
void *calloc(size_t nmemb, size_t size);

/* Output */
int putchar(int c);
int puts(const char *s);
void exit(int status);

#endif /* SETUN_MINILIBC_H */
