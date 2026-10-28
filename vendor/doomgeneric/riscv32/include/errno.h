/* Minimal errno for the Setun-Doom RV32IM port. */
#ifndef SETUN_ERRNO_H
#define SETUN_ERRNO_H

extern int errno;

#define EPERM 1
#define ENOENT 2
#define EISDIR 21
#define EINTR 4
#define EIO 5
#define EBADF 9
#define ENOMEM 12
#define EACCES 13
#define EEXIST 17
#define EINVAL 22
#define ENOSPC 28

#endif
