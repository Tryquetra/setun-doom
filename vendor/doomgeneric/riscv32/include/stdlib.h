/* Minimal freestanding stdlib for the Setun-Doom RV32IM port. */
#ifndef SETUN_STDLIB_H
#define SETUN_STDLIB_H

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX 0x7FFFFFFF

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);

int atoi(const char *s);
long atol(const char *s);
float atof(const char *s);
long strtol(const char *s, char **end, int base);
unsigned long strtoul(const char *s, char **end, int base);

int abs(int v);
long labs(long v);
int rand(void);
void srand(unsigned int seed);
void qsort(void *base, size_t count, size_t size,
           int (*compare)(const void *, const void *));

void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));
int atexit(void (*fn)(void));
char *getenv(const char *name);
int system(const char *command);

#endif
