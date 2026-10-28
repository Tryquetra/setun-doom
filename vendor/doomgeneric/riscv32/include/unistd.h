/* Minimal freestanding unistd.h for the Setun-Doom RV32IM port. */
#ifndef SETUN_UNISTD_H
#define SETUN_UNISTD_H

#include <stddef.h>

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

long read(int fd, void *buf, size_t count);
long write(int fd, const void *buf, size_t count);
int close(int fd);
long lseek(int fd, long offset, int whence);
int unlink(const char *path);
unsigned int sleep(unsigned int seconds);
int usleep(unsigned int microseconds);
int getpid(void);
void _exit(int status) __attribute__((noreturn));

#endif
