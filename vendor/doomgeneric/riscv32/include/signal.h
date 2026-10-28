/* Minimal signal.h for the Setun-Doom RV32IM port: no signals on bare metal. */
#ifndef SETUN_SIGNAL_H
#define SETUN_SIGNAL_H

typedef void (*sighandler_t)(int);

#define SIGINT 2
#define SIGTERM 15
#define SIG_DFL ((sighandler_t)0)
#define SIG_IGN ((sighandler_t)1)

sighandler_t signal(int signum, sighandler_t handler);
int raise(int signum);

#endif
