// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Minimal freestanding C library for the RISC-V guest that runs Doom on the Setun-Doom DBT.
//!
//! Implements the subset of libc that `doomgeneric` needs: memory and string routines, a bump
//! allocator, a formatter, `sscanf`, character classification, basic math, and a dummy `FILE`
//! layer (WAD access goes through `w_file_riscv.c` and the host syscalls instead).

#include <ctype.h>
#include <errno.h>
#include <math.h>
#include <signal.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

#include "setun_syscall.h"

int errno;

/* ------------------------------------------------------------------------- */
/* Memory                                                                    */
/* ------------------------------------------------------------------------- */

extern char _heap_start[];
extern char _heap_end[];

static char *heap_cursor = 0;
static char *heap_limit = 0;

typedef struct {
    size_t size;
    size_t reserved;
} alloc_header_t;

static void heap_init(void)
{
    if (heap_cursor == 0) {
        heap_cursor = _heap_start;
        heap_limit = _heap_end;
    }
}

void *malloc(size_t size)
{
    heap_init();
    size_t aligned = (size + 7u) & ~(size_t)7u;
    size_t total = aligned + sizeof(alloc_header_t);
    if (heap_cursor + total > heap_limit) {
        return 0;
    }
    alloc_header_t *header = (alloc_header_t *)heap_cursor;
    header->size = aligned;
    header->reserved = 0;
    void *block = heap_cursor + sizeof(alloc_header_t);
    heap_cursor += total;
    return block;
}

void free(void *ptr)
{
    (void)ptr;
}

void *calloc(size_t count, size_t size)
{
    size_t total = count * size;
    void *block = malloc(total);
    if (block != 0) {
        memset(block, 0, total);
    }
    return block;
}

void *realloc(void *ptr, size_t size)
{
    if (ptr == 0) {
        return malloc(size);
    }
    alloc_header_t *header = (alloc_header_t *)((char *)ptr - sizeof(alloc_header_t));
    size_t previous = header->size;
    if (size <= previous) {
        return ptr;
    }
    void *fresh = malloc(size);
    if (fresh == 0) {
        return 0;
    }
    memcpy(fresh, ptr, previous);
    return fresh;
}

/* ------------------------------------------------------------------------- */
/* Strings and memory primitives                                             */
/* ------------------------------------------------------------------------- */

void *memcpy(void *dst, const void *src, size_t count)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    /* Word-wise copy when both sides are 4-byte aligned: the guest is byte-addressed, so each
     * 32-bit store covers four Setun words and cuts the instruction count of frame copies. */
    if (((uintptr_t)d & 3u) == 0u && ((uintptr_t)s & 3u) == 0u) {
        while (count >= 4u) {
            *(unsigned int *)d = *(const unsigned int *)s;
            d += 4;
            s += 4;
            count -= 4u;
        }
    }
    while (count--) {
        *d++ = *s++;
    }
    return dst;
}

void *memmove(void *dst, const void *src, size_t count)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    if ((d >= s + count) || (d < s)) {
        return memcpy(dst, src, count);
    }
    if (d > s) {
        d += count;
        s += count;
        while (count--) {
            *--d = *--s;
        }
    }
    return dst;
}

void *memset(void *dst, int value, size_t count)
{
    unsigned char *d = dst;
    if (((uintptr_t)d & 3u) == 0u) {
        unsigned int pattern = (unsigned char)value;
        pattern |= pattern << 8;
        pattern |= pattern << 16;
        while (count >= 4u) {
            *(unsigned int *)d = pattern;
            d += 4;
            count -= 4u;
        }
    }
    while (count--) {
        *d++ = (unsigned char)value;
    }
    return dst;
}

int memcmp(const void *lhs, const void *rhs, size_t count)
{
    const unsigned char *a = lhs;
    const unsigned char *b = rhs;
    while (count--) {
        if (*a != *b) {
            return (int)*a - (int)*b;
        }
        a++;
        b++;
    }
    return 0;
}

size_t strlen(const char *s)
{
    const char *p = s;
    while (*p) {
        p++;
    }
    return (size_t)(p - s);
}

char *strcpy(char *dst, const char *src)
{
    char *d = dst;
    while ((*d++ = *src++) != 0) {
    }
    return dst;
}

char *strncpy(char *dst, const char *src, size_t count)
{
    char *d = dst;
    while (count && *src) {
        *d++ = *src++;
        count--;
    }
    while (count--) {
        *d++ = 0;
    }
    return dst;
}

char *strcat(char *dst, const char *src)
{
    strcpy(dst + strlen(dst), src);
    return dst;
}

char *strncat(char *dst, const char *src, size_t count)
{
    char *d = dst + strlen(dst);
    while (count-- && *src) {
        *d++ = *src++;
    }
    *d = 0;
    return dst;
}

int strcmp(const char *lhs, const char *rhs)
{
    while (*lhs && *lhs == *rhs) {
        lhs++;
        rhs++;
    }
    return (int)(unsigned char)*lhs - (int)(unsigned char)*rhs;
}

int strncmp(const char *lhs, const char *rhs, size_t count)
{
    while (count && *lhs && *lhs == *rhs) {
        lhs++;
        rhs++;
        count--;
    }
    if (count == 0) {
        return 0;
    }
    return (int)(unsigned char)*lhs - (int)(unsigned char)*rhs;
}

char *strchr(const char *s, int c)
{
    for (;; s++) {
        if (*s == (char)c) {
            return (char *)s;
        }
        if (*s == 0) {
            return 0;
        }
    }
}

char *strrchr(const char *s, int c)
{
    const char *last = 0;
    for (;; s++) {
        if (*s == (char)c) {
            last = s;
        }
        if (*s == 0) {
            return (char *)last;
        }
    }
}

char *strstr(const char *haystack, const char *needle)
{
    size_t needle_len = strlen(needle);
    if (needle_len == 0) {
        return (char *)haystack;
    }
    for (; *haystack; haystack++) {
        if (strncmp(haystack, needle, needle_len) == 0) {
            return (char *)haystack;
        }
    }
    return 0;
}

char *strdup(const char *s)
{
    size_t len = strlen(s) + 1;
    char *copy = malloc(len);
    if (copy != 0) {
        memcpy(copy, s, len);
    }
    return copy;
}

char *strerror(int errnum)
{
    (void)errnum;
    return "error";
}

static int lower_ascii(int c)
{
    if (c >= 'A' && c <= 'Z') {
        return c + ('a' - 'A');
    }
    return c;
}

int strcasecmp(const char *lhs, const char *rhs)
{
    while (*lhs && lower_ascii(*lhs) == lower_ascii(*rhs)) {
        lhs++;
        rhs++;
    }
    return lower_ascii((unsigned char)*lhs) - lower_ascii((unsigned char)*rhs);
}

int strncasecmp(const char *lhs, const char *rhs, size_t count)
{
    while (count && *lhs && lower_ascii(*lhs) == lower_ascii(*rhs)) {
        lhs++;
        rhs++;
        count--;
    }
    if (count == 0) {
        return 0;
    }
    return lower_ascii((unsigned char)*lhs) - lower_ascii((unsigned char)*rhs);
}

/* ------------------------------------------------------------------------- */
/* ctype                                                                     */
/* ------------------------------------------------------------------------- */

int isspace(int c)
{
    return c == ' ' || c == '\t' || c == '\n' || c == '\v' || c == '\f' || c == '\r';
}

int isdigit(int c)
{
    return c >= '0' && c <= '9';
}

int isxdigit(int c)
{
    return isdigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
}

int isalpha(int c)
{
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
}

int isalnum(int c)
{
    return isalpha(c) || isdigit(c);
}

int isprint(int c)
{
    return c >= 32 && c < 127;
}

int isgraph(int c)
{
    return c > 32 && c < 127;
}

int ispunct(int c)
{
    return isgraph(c) && !isalnum(c);
}

int iscntrl(int c)
{
    return (c >= 0 && c < 32) || c == 127;
}

int isupper(int c)
{
    return c >= 'A' && c <= 'Z';
}

int islower(int c)
{
    return c >= 'a' && c <= 'z';
}

int toupper(int c)
{
    return islower(c) ? c - ('a' - 'A') : c;
}

int tolower(int c)
{
    return isupper(c) ? c + ('a' - 'A') : c;
}

/* ------------------------------------------------------------------------- */
/* Conversions                                                               */
/* ------------------------------------------------------------------------- */

int abs(int v)
{
    return v < 0 ? -v : v;
}

long labs(long v)
{
    return v < 0 ? -v : v;
}

long strtol(const char *s, char **end, int base)
{
    long value = 0;
    int negative = 0;
    while (isspace((unsigned char)*s)) {
        s++;
    }
    if (*s == '-' || *s == '+') {
        negative = (*s == '-');
        s++;
    }
    if (base == 0) {
        base = 10;
        if (*s == '0') {
            base = 8;
            s++;
            if (lower_ascii(*s) == 'x') {
                base = 16;
                s++;
            }
        }
    }
    for (;;) {
        int digit;
        if (isdigit((unsigned char)*s)) {
            digit = *s - '0';
        } else if (isalpha((unsigned char)*s)) {
            digit = lower_ascii(*s) - 'a' + 10;
        } else {
            break;
        }
        if (digit >= base) {
            break;
        }
        value = value * base + digit;
        s++;
    }
    if (end != 0) {
        *end = (char *)s;
    }
    return negative ? -value : value;
}

unsigned long strtoul(const char *s, char **end, int base)
{
    return (unsigned long)strtol(s, end, base);
}

int atoi(const char *s)
{
    return (int)strtol(s, 0, 10);
}

long atol(const char *s)
{
    return strtol(s, 0, 10);
}

float atof(const char *s)
{
    float value = 0.0f;
    int negative = 0;
    float scale = 1.0f;
    int in_fraction = 0;
    while (isspace((unsigned char)*s)) {
        s++;
    }
    if (*s == '-' || *s == '+') {
        negative = (*s == '-');
        s++;
    }
    for (;; s++) {
        if (*s == '.') {
            in_fraction = 1;
            continue;
        }
        if (!isdigit((unsigned char)*s)) {
            break;
        }
        if (in_fraction) {
            scale *= 0.1f;
            value += (*s - '0') * scale;
        } else {
            value = value * 10.0f + (float)(*s - '0');
        }
    }
    return negative ? -value : value;
}

/* ------------------------------------------------------------------------- */
/* Random and sorting                                                        */
/* ------------------------------------------------------------------------- */

static unsigned int random_state = 1u;

int rand(void)
{
    random_state = random_state * 1103515245u + 12345u;
    return (int)(random_state & 0x7FFFFFFFu);
}

void srand(unsigned int seed)
{
    random_state = seed;
}

void qsort(void *base, size_t count, size_t size,
           int (*compare)(const void *, const void *))
{
    unsigned char *bytes = base;
    unsigned char temp[64];
    for (size_t i = 1; i < count; i++) {
        size_t j = i;
        while (j > 0 && compare(bytes + (j - 1) * size, bytes + j * size) > 0) {
            memcpy(temp, bytes + (j - 1) * size, size);
            memcpy(bytes + (j - 1) * size, bytes + j * size, size);
            memcpy(bytes + j * size, temp, size);
            j--;
        }
    }
}

/* ------------------------------------------------------------------------- */
/* Exit                                                                      */
/* ------------------------------------------------------------------------- */

#define ATEXIT_MAX 8
static void (*atexit_handlers[ATEXIT_MAX])(void);
static int atexit_count = 0;

int atexit(void (*fn)(void))
{
    if (atexit_count >= ATEXIT_MAX) {
        return -1;
    }
    atexit_handlers[atexit_count++] = fn;
    return 0;
}

void exit(int status)
{
    while (atexit_count > 0) {
        atexit_handlers[--atexit_count]();
    }
    setun_exit(status);
}

void abort(void)
{
    setun_exit(134);
}

void _exit(int status)
{
    setun_exit(status);
}

char *getenv(const char *name)
{
    (void)name;
    return 0;
}

int system(const char *command)
{
    (void)command;
    return -1;
}

void __assert_fail(const char *expr, const char *file, int line)
{
    char buffer[192];
    int len = snprintf(buffer, sizeof(buffer), "assert(%s) failed at %s:%d\n", expr, file, line);
    if (len > 0) {
        setun_write(buffer, len);
    }
    setun_exit(134);
}

/* ------------------------------------------------------------------------- */
/* Formatting                                                                */
/* ------------------------------------------------------------------------- */

typedef struct {
    char *buffer;
    size_t capacity;
    size_t written;
} format_sink_t;

static void sink_char(format_sink_t *sink, char c)
{
    if (sink->written + 1 < sink->capacity) {
        sink->buffer[sink->written] = c;
    }
    sink->written++;
}

static void sink_pad(format_sink_t *sink, char c, int count)
{
    while (count-- > 0) {
        sink_char(sink, c);
    }
}

static void sink_string(format_sink_t *sink, const char *s, int width, int left, int zero)
{
    int len = (int)strlen(s);
    int pad = width > len ? width - len : 0;
    if (!left) {
        sink_pad(sink, zero ? '0' : ' ', pad);
    }
    while (*s) {
        sink_char(sink, *s++);
    }
    if (left) {
        sink_pad(sink, ' ', pad);
    }
}

static void sink_unsigned(format_sink_t *sink, unsigned long value, unsigned base,
                          int uppercase, int width, int left, int zero, int negative,
                          int precision)
{
    char digits[32];
    const char *alphabet = uppercase ? "0123456789ABCDEF" : "0123456789abcdef";
    int count = 0;
    if (value == 0) {
        if (precision != 0) {
            digits[count++] = '0';
        }
    }
    while (value > 0) {
        digits[count++] = alphabet[value % base];
        value /= base;
    }
    while (precision > count) {
        digits[count++] = '0';
    }
    if (negative) {
        digits[count++] = '-';
    }
    int pad = width > count ? width - count : 0;
    if (!left) {
        sink_pad(sink, zero && precision < 0 ? '0' : ' ', pad);
    }
    while (count > 0) {
        sink_char(sink, digits[--count]);
    }
    if (left) {
        sink_pad(sink, ' ', pad);
    }
}

extern float __truncdfsf2(double value);

static void sink_float(format_sink_t *sink, float value, int width, int precision)
{
    char buffer[48];
    int negative = value < 0.0f;
    if (negative) {
        value = -value;
    }
    unsigned long integer = (unsigned long)value;
    float fraction = value - (float)integer;
    unsigned long scaled = 1;
    for (int i = 0; i < precision; i++) {
        scaled *= 10;
    }
    unsigned long frac_digits = (unsigned long)(fraction * (float)scaled + 0.5f);
    if (frac_digits >= scaled) {
        integer += 1;
        frac_digits -= scaled;
    }
    snprintf(buffer, sizeof(buffer), "%s%lu.%0*lu", negative ? "-" : "", integer, precision,
             frac_digits);
    sink_string(sink, buffer, width, 0, 0);
}

int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap)
{
    format_sink_t sink = {buf, size, 0};
    for (const char *p = fmt; *p; p++) {
        if (*p != '%') {
            sink_char(&sink, *p);
            continue;
        }
        p++;
        int left = 0;
        int zero = 0;
        int width = 0;
        int precision = 6;
        int have_precision = 0;
        while (*p == '-' || *p == '0' || *p == '+' || *p == ' ' || *p == '#') {
            if (*p == '-') {
                left = 1;
            } else if (*p == '0') {
                zero = 1;
            }
            p++;
        }
        while (isdigit((unsigned char)*p)) {
            width = width * 10 + (*p - '0');
            p++;
        }
        if (*p == '.') {
            p++;
            precision = 0;
            have_precision = 1;
            while (isdigit((unsigned char)*p)) {
                precision = precision * 10 + (*p - '0');
                p++;
            }
        }
        while (*p == 'l' || *p == 'h' || *p == 'z' || *p == 't') {
            p++;
        }
        switch (*p) {
            case 'd':
            case 'i': {
                long value = va_arg(ap, long);
                int negative = value < 0;
                unsigned long magnitude = negative ? (unsigned long)(-value) : (unsigned long)value;
                sink_unsigned(&sink, magnitude, 10, 0, width, left, zero, negative,
                              have_precision ? precision : -1);
                break;
            }
            case 'u':
                sink_unsigned(&sink, va_arg(ap, unsigned long), 10, 0, width, left, zero, 0,
                              have_precision ? precision : -1);
                break;
            case 'x':
                sink_unsigned(&sink, va_arg(ap, unsigned long), 16, 0, width, left, zero, 0,
                              have_precision ? precision : -1);
                break;
            case 'X':
                sink_unsigned(&sink, va_arg(ap, unsigned long), 16, 1, width, left, zero, 0,
                              have_precision ? precision : -1);
                break;
            case 'o':
                sink_unsigned(&sink, va_arg(ap, unsigned long), 8, 0, width, left, zero, 0,
                              have_precision ? precision : -1);
                break;
            case 'p': {
                sink_string(&sink, "0x", 0, 0, 0);
                sink_unsigned(&sink, (unsigned long)(uintptr_t)va_arg(ap, void *), 16, 0, 8, 0, 1,
                              0, -1);
                break;
            }
            case 'c': {
                char c = (char)va_arg(ap, int);
                sink_pad(&sink, ' ', width > 1 ? width - 1 : 0);
                sink_char(&sink, c);
                break;
            }
            case 's': {
                const char *s = va_arg(ap, const char *);
                sink_string(&sink, s != 0 ? s : "(null)", width, left, zero);
                break;
            }
            case 'f': {
                double promoted = va_arg(ap, double);
                sink_float(&sink, __truncdfsf2(promoted), width, have_precision ? precision : 6);
                break;
            }
            case '%':
                sink_char(&sink, '%');
                break;
            case 0:
                p--;
                break;
            default:
                sink_char(&sink, '%');
                sink_char(&sink, *p);
                break;
        }
    }
    if (sink.capacity > 0) {
        size_t end = sink.written < sink.capacity ? sink.written : sink.capacity - 1;
        sink.buffer[end] = 0;
    }
    return (int)sink.written;
}

int snprintf(char *buf, size_t size, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int result = vsnprintf(buf, size, fmt, ap);
    va_end(ap);
    return result;
}

int sprintf(char *buf, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int result = vsnprintf(buf, (size_t)-1, fmt, ap);
    va_end(ap);
    return result;
}

int vprintf(const char *fmt, va_list ap)
{
    char buffer[1024];
    int len = vsnprintf(buffer, sizeof(buffer), fmt, ap);
    if (len > 0) {
        int to_write = len < (int)sizeof(buffer) - 1 ? len : (int)sizeof(buffer) - 1;
        setun_write(buffer, to_write);
    }
    return len;
}

int printf(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int result = vprintf(fmt, ap);
    va_end(ap);
    return result;
}

int vfprintf(FILE *stream, const char *fmt, va_list ap)
{
    (void)stream;
    return vprintf(fmt, ap);
}

int fprintf(FILE *stream, const char *fmt, ...)
{
    (void)stream;
    va_list ap;
    va_start(ap, fmt);
    int result = vprintf(fmt, ap);
    va_end(ap);
    return result;
}

int puts(const char *s)
{
    int len = (int)strlen(s);
    setun_write(s, len);
    setun_write("\n", 1);
    return len + 1;
}

int putchar(int c)
{
    char ch = (char)c;
    setun_write(&ch, 1);
    return c;
}

int fputc(int c, FILE *stream)
{
    (void)stream;
    return putchar(c);
}

int fputs(const char *s, FILE *stream)
{
    (void)stream;
    return (int)strlen(s);
}

/* ------------------------------------------------------------------------- */
/* Files: WAD access uses w_file_riscv.c; config and saves are discarded.    */
/* ------------------------------------------------------------------------- */

struct setun_file {
    int used;
    long position;
};

static struct setun_file dummy_stdin;
static struct setun_file dummy_stdout;
static struct setun_file dummy_stderr;
static struct setun_file dummy_config;

FILE *stdin = &dummy_stdin;
FILE *stdout = &dummy_stdout;
FILE *stderr = &dummy_stderr;

FILE *fopen(const char *path, const char *mode)
{
    (void)path;
    (void)mode;
    dummy_config.used = 1;
    dummy_config.position = 0;
    return &dummy_config;
}

int fclose(FILE *stream)
{
    if (stream != 0) {
        stream->used = 0;
    }
    return 0;
}

size_t fread(void *ptr, size_t size, size_t count, FILE *stream)
{
    (void)ptr;
    (void)size;
    (void)count;
    (void)stream;
    return 0;
}

size_t fwrite(const void *ptr, size_t size, size_t count, FILE *stream)
{
    (void)ptr;
    (void)stream;
    return size * count;
}

int fseek(FILE *stream, long offset, int whence)
{
    (void)whence;
    if (stream != 0) {
        stream->position = offset;
    }
    return 0;
}

long ftell(FILE *stream)
{
    return stream != 0 ? stream->position : 0;
}

int fflush(FILE *stream)
{
    (void)stream;
    return 0;
}

int feof(FILE *stream)
{
    (void)stream;
    return 1;
}

int fgetc(FILE *stream)
{
    (void)stream;
    return EOF;
}

char *fgets(char *s, int size, FILE *stream)
{
    (void)stream;
    if (size > 0) {
        s[0] = 0;
    }
    return 0;
}

int remove(const char *path)
{
    (void)path;
    return 0;
}

int rename(const char *old_path, const char *new_path)
{
    (void)old_path;
    (void)new_path;
    return 0;
}

int sscanf(const char *str, const char *fmt, ...)
{
    va_list ap;
    int assigned = 0;
    va_start(ap, fmt);
    while (*fmt) {
        if (isspace((unsigned char)*fmt)) {
            while (isspace((unsigned char)*str)) {
                str++;
            }
            fmt++;
            continue;
        }
        if (*fmt != '%') {
            if (*str != *fmt) {
                break;
            }
            str++;
            fmt++;
            continue;
        }
        fmt++;
        int width = 0;
        while (isdigit((unsigned char)*fmt)) {
            width = width * 10 + (*fmt - '0');
            fmt++;
        }
        if (*fmt == 'l') {
            fmt++;
        }
        switch (*fmt) {
            case 'd':
            case 'i':
            case 'u': {
                int *out = va_arg(ap, int *);
                int negative = 0;
                int value = 0;
                while (isspace((unsigned char)*str)) {
                    str++;
                }
                if (*str == '-' || *str == '+') {
                    negative = (*str == '-');
                    str++;
                }
                while (isdigit((unsigned char)*str) && (width == 0 || width-- > 0)) {
                    value = value * 10 + (*str++ - '0');
                }
                *out = negative ? -value : value;
                assigned++;
                break;
            }
            case 'x': {
                unsigned int *out = va_arg(ap, unsigned int *);
                unsigned int value = 0;
                while (isxdigit((unsigned char)*str)) {
                    int digit = isdigit((unsigned char)*str)
                                    ? *str - '0'
                                    : lower_ascii(*str) - 'a' + 10;
                    value = value * 16 + (unsigned)digit;
                    str++;
                }
                *out = value;
                assigned++;
                break;
            }
            case 'f': {
                float *out = va_arg(ap, float *);
                *out = (float)atof(str);
                while (*str && !isspace((unsigned char)*str)) {
                    str++;
                }
                assigned++;
                break;
            }
            case 'c': {
                char *out = va_arg(ap, char *);
                *out = *str++;
                assigned++;
                break;
            }
            case 's': {
                char *out = va_arg(ap, char *);
                while (isspace((unsigned char)*str)) {
                    str++;
                }
                while (*str && !isspace((unsigned char)*str)) {
                    *out++ = *str++;
                }
                *out = 0;
                assigned++;
                break;
            }
            case '%':
                if (*str == '%') {
                    str++;
                }
                break;
            default:
                break;
        }
        fmt++;
    }
    va_end(ap);
    return assigned;
}

/* ------------------------------------------------------------------------- */
/* Math                                                                      */
/* ------------------------------------------------------------------------- */

float fabs(float x)
{
    return x < 0.0f ? -x : x;
}

float floor(float x)
{
    long truncated = (long)x;
    if ((float)truncated > x) {
        truncated -= 1;
    }
    return (float)truncated;
}

float ceil(float x)
{
    long truncated = (long)x;
    if ((float)truncated < x) {
        truncated += 1;
    }
    return (float)truncated;
}

float sqrt(float x)
{
    if (x <= 0.0f) {
        return 0.0f;
    }
    float guess = x > 1.0f ? x : 1.0f;
    for (int i = 0; i < 24; i++) {
        guess = 0.5f * (guess + x / guess);
    }
    return guess;
}

float fmod(float x, float y)
{
    if (y == 0.0f) {
        return 0.0f;
    }
    long quotient = (long)(x / y);
    return x - (float)quotient * y;
}

float pow(float base, float exponent)
{
    long whole = (long)exponent;
    float result = 1.0f;
    int negative = whole < 0;
    if (negative) {
        whole = -whole;
    }
    for (long i = 0; i < whole; i++) {
        result *= base;
    }
    if (negative) {
        result = 1.0f / result;
    }
    float fraction = exponent - (float)(long)exponent;
    if (fraction != 0.0f) {
        result *= 1.0f + fraction * (base - 1.0f);
    }
    return result;
}

static float reduce_angle(float x)
{
    const float two_pi = 6.28318530717958647692f;
    while (x > M_PI) {
        x -= two_pi;
    }
    while (x < -M_PI) {
        x += two_pi;
    }
    return x;
}

float sin(float x)
{
    x = reduce_angle(x);
    float term = x;
    float sum = x;
    for (int i = 1; i < 8; i++) {
        term *= -x * x / (float)((2 * i) * (2 * i + 1));
        sum += term;
    }
    return sum;
}

float cos(float x)
{
    return sin(x + M_PI / 2.0f);
}

float tan(float x)
{
    float c = cos(x);
    if (c > -1e-6f && c < 1e-6f) {
        return 0.0f;
    }
    return sin(x) / c;
}

float atan(float x)
{
    int inverted = 0;
    if (x > 1.0f) {
        x = 1.0f / x;
        inverted = 1;
    } else if (x < -1.0f) {
        x = 1.0f / x;
        inverted = -1;
    }
    float term = x;
    float sum = x;
    float x2 = x * x;
    for (int i = 1; i < 20; i++) {
        term *= -x2;
        sum += term / (float)(2 * i + 1);
    }
    if (inverted == 1) {
        return M_PI / 2.0f - sum;
    }
    if (inverted == -1) {
        return -M_PI / 2.0f - sum;
    }
    return sum;
}

float atan2(float y, float x)
{
    if (x > 0.0f) {
        return atan(y / x);
    }
    if (x < 0.0f) {
        return y >= 0.0f ? atan(y / x) + M_PI : atan(y / x) - M_PI;
    }
    return y >= 0.0f ? M_PI / 2.0f : -M_PI / 2.0f;
}

/* ------------------------------------------------------------------------- */
/* Time, signals, syscall file wrappers                                      */
/* ------------------------------------------------------------------------- */

time_t time(time_t *out)
{
    time_t now = (time_t)(setun_ticks_ms() / 1000u);
    if (out != 0) {
        *out = now;
    }
    return now;
}

int gettimeofday(struct timeval *tv, struct timezone *tz)
{
    (void)tz;
    if (tv != 0) {
        unsigned int ms = setun_ticks_ms();
        tv->tv_sec = (time_t)(ms / 1000u);
        tv->tv_usec = (suseconds_t)((ms % 1000u) * 1000u);
    }
    return 0;
}

sighandler_t signal(int signum, sighandler_t handler)
{
    (void)signum;
    return handler;
}

int raise(int signum)
{
    (void)signum;
    return 0;
}

unsigned int sleep(unsigned int seconds)
{
    setun_sleep_ms((int32_t)(seconds * 1000u));
    return 0;
}

int usleep(unsigned int microseconds)
{
    setun_sleep_ms((int32_t)(microseconds / 1000u));
    return 0;
}

int getpid(void)
{
    return 1;
}

long read(int fd, void *buf, size_t count)
{
    (void)fd;
    (void)buf;
    (void)count;
    return 0;
}

long write(int fd, const void *buf, size_t count)
{
    (void)fd;
    return setun_write(buf, (int32_t)count);
}

int close(int fd)
{
    (void)fd;
    return 0;
}

long lseek(int fd, long offset, int whence)
{
    (void)fd;
    (void)whence;
    return offset;
}

int unlink(const char *path)
{
    (void)path;
    return 0;
}

int open(const char *path, int flags, ...)
{
    (void)path;
    (void)flags;
    return -1;
}

int creat(const char *path, int mode)
{
    (void)path;
    (void)mode;
    return -1;
}

int mkdir(const char *path, mode_t mode)
{
    (void)path;
    (void)mode;
    return -1;
}

int stat(const char *path, struct stat *buf)
{
    (void)path;
    if (buf != 0) {
        buf->st_mode = 0;
        buf->st_size = 0;
        buf->st_mtime = 0;
    }
    return -1;
}
