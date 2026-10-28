/* Case-insensitive string helpers for the Setun-Doom RV32IM port. */
#ifndef SETUN_STRINGS_H
#define SETUN_STRINGS_H

#include <stddef.h>

int strcasecmp(const char *lhs, const char *rhs);
int strncasecmp(const char *lhs, const char *rhs, size_t count);

#endif
