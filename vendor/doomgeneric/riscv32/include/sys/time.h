/* Minimal sys/time.h for the Setun-Doom RV32IM port. */
#ifndef SETUN_SYS_TIME_H
#define SETUN_SYS_TIME_H

typedef long time_t;
typedef long suseconds_t;

struct timeval {
    time_t tv_sec;
    suseconds_t tv_usec;
};

struct timezone {
    int tz_minuteswest;
    int tz_dsttime;
};

int gettimeofday(struct timeval *tv, struct timezone *tz);

#endif
