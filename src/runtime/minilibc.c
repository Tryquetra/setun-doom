/*
 * minilibc.c — Freestanding C runtime implementation for Setun-Doom
 * Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
 * SPDX-License-Identifier: MIT
 */

#include "minilibc.h"

/* Memory operations */

void *memcpy(void *dest, const void *src, size_t n) {
    char *d = (char *)dest;
    const char *s = (const char *)src;
    for (size_t i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dest;
}

void *memset(void *s, int c, size_t n) {
    char *p = (char *)s;
    for (size_t i = 0; i < n; i++) {
        p[i] = (char)c;
    }
    return s;
}

void *memmove(void *dest, const void *src, size_t n) {
    char *d = (char *)dest;
    const char *s = (const char *)src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) {
            d[i] = s[i];
        }
    } else if (d > s) {
        for (size_t i = n; i > 0; i--) {
            d[i - 1] = s[i - 1];
        }
    }
    return dest;
}

int memcmp(const void *s1, const void *s2, size_t n) {
    const unsigned char *p1 = (const unsigned char *)s1;
    const unsigned char *p2 = (const unsigned char *)s2;
    for (size_t i = 0; i < n; i++) {
        if (p1[i] != p2[i]) {
            return (int)p1[i] - (int)p2[i];
        }
    }
    return 0;
}

/* String operations */

size_t strlen(const char *s) {
    size_t len = 0;
    while (s[len] != '\0') {
        len++;
    }
    return len;
}

char *strcpy(char *dest, const char *src) {
    size_t i = 0;
    while (src[i] != '\0') {
        dest[i] = src[i];
        i++;
    }
    dest[i] = '\0';
    return dest;
}

char *strncpy(char *dest, const char *src, size_t n) {
    size_t i = 0;
    for (; i < n && src[i] != '\0'; i++) {
        dest[i] = src[i];
    }
    for (; i < n; i++) {
        dest[i] = '\0';
    }
    return dest;
}

char *strcat(char *dest, const char *src) {
    size_t dlen = strlen(dest);
    size_t i = 0;
    while (src[i] != '\0') {
        dest[dlen + i] = src[i];
        i++;
    }
    dest[dlen + i] = '\0';
    return dest;
}

int strcmp(const char *s1, const char *s2) {
    size_t i = 0;
    while (s1[i] != '\0' && s1[i] == s2[i]) {
        i++;
    }
    return (int)(unsigned char)s1[i] - (int)(unsigned char)s2[i];
}

int strncmp(const char *s1, const char *s2, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (s1[i] != s2[i] || s1[i] == '\0') {
            return (int)(unsigned char)s1[i] - (int)(unsigned char)s2[i];
        }
    }
    return 0;
}

char *strchr(const char *s, int c) {
    while (*s != '\0') {
        if (*s == (char)c) {
            return (char *)s;
        }
        s++;
    }
    return (c == '\0') ? (char *)s : NULL;
}

char *strrchr(const char *s, int c) {
    const char *last = NULL;
    while (*s != '\0') {
        if (*s == (char)c) {
            last = s;
        }
        s++;
    }
    return (c == '\0') ? (char *)s : (char *)last;
}

char *strstr(const char *haystack, const char *needle) {
    if (!*needle) {
        return (char *)haystack;
    }
    for (size_t i = 0; haystack[i] != '\0'; i++) {
        size_t j = 0;
        while (needle[j] != '\0' && haystack[i + j] == needle[j]) {
            j++;
        }
        if (needle[j] == '\0') {
            return (char *)(haystack + i);
        }
    }
    return NULL;
}

/* Character & parsing */

int toupper(int c) {
    if (c >= 'a' && c <= 'z') {
        return c - ('a' - 'A');
    }
    return c;
}

int tolower(int c) {
    if (c >= 'A' && c <= 'Z') {
        return c + ('a' - 'A');
    }
    return c;
}

int isspace(int c) {
    return (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v');
}

int isdigit(int c) {
    return (c >= '0' && c <= '9');
}

int atoi(const char *nptr) {
    int res = 0;
    int sign = 1;
    while (isspace(*nptr)) {
        nptr++;
    }
    if (*nptr == '-') {
        sign = -1;
        nptr++;
    } else if (*nptr == '+') {
        nptr++;
    }
    while (isdigit(*nptr)) {
        res = res * 10 + (*nptr - '0');
        nptr++;
    }
    return res * sign;
}

int abs(int j) {
    return (j < 0) ? -j : j;
}

/* Dynamic allocation: Bump allocator in RAM region 200_000..499_999 */
#define SETUN_HEAP_BASE 200000
static size_t g_heap_top = SETUN_HEAP_BASE;

void *malloc(size_t size) {
    if (size == 0) {
        return NULL;
    }
    /* Align to 4 bytes */
    size_t aligned_size = (size + 3) & ~((size_t)3);
    void *ptr = (void *)g_heap_top;
    g_heap_top += aligned_size;
    return ptr;
}

void free(void *ptr) {
    /* No-op in freestanding bump allocator */
    (void)ptr;
}

void *realloc(void *ptr, size_t size) {
    if (!ptr) {
        return malloc(size);
    }
    void *new_ptr = malloc(size);
    if (new_ptr) {
        memcpy(new_ptr, ptr, size);
    }
    return new_ptr;
}

void *calloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *ptr = malloc(total);
    if (ptr) {
        memset(ptr, 0, total);
    }
    return ptr;
}

/* Console & exit */

int putchar(int c) {
    /* Output character (in future connected to MMIO terminal) */
    return c;
}

int puts(const char *s) {
    while (*s) {
        putchar(*s++);
    }
    putchar('\n');
    return 0;
}

void exit(int status) {
    (void)status;
    /* Loops indefinitely or triggers trap on termination */
    while (1) {}
}
