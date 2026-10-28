/* Minimal fcntl.h for the Setun-Doom RV32IM port. */
#ifndef SETUN_FCNTL_H
#define SETUN_FCNTL_H

#define O_RDONLY 0
#define O_WRONLY 1
#define O_RDWR 2
#define O_CREAT 0100
#define O_TRUNC 01000

int open(const char *path, int flags, ...);
int creat(const char *path, int mode);

#endif
