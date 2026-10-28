// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT
//
// 32-bit soft multiply/divide helpers for the RV32I build of the Doom guest.
// Clang emits calls to these when targeting `-march=rv32i` (no M extension).

#include <stdint.h>

int32_t __mulsi3(int32_t a, int32_t b)
{
    uint32_t ua = (uint32_t)a;
    uint32_t ub = (uint32_t)b;
    uint32_t result = 0;

    while (ub != 0) {
        if (ub & 1u) {
            result += ua;
        }
        ua <<= 1;
        ub >>= 1;
    }
    return (int32_t)result;
}

uint32_t __udivsi3(uint32_t numerator, uint32_t denominator)
{
    if (denominator == 0) {
        return 0xFFFFFFFFu;
    }
    uint32_t quotient = 0;
    uint32_t remainder = 0;
    for (int bit = 31; bit >= 0; --bit) {
        remainder = (remainder << 1) | ((numerator >> bit) & 1u);
        if (remainder >= denominator) {
            remainder -= denominator;
            quotient |= (1u << bit);
        }
    }
    return quotient;
}

uint32_t __umodsi3(uint32_t numerator, uint32_t denominator)
{
    if (denominator == 0) {
        return numerator;
    }
    uint32_t remainder = 0;
    for (int bit = 31; bit >= 0; --bit) {
        remainder = (remainder << 1) | ((numerator >> bit) & 1u);
        if (remainder >= denominator) {
            remainder -= denominator;
        }
    }
    return remainder;
}

int32_t __divsi3(int32_t numerator, int32_t denominator)
{
    if (denominator == 0) {
        return -1;
    }
    if (denominator == -1) {
        return (int32_t)(0u - (uint32_t)numerator);
    }
    uint32_t negative = ((uint32_t)numerator ^ (uint32_t)denominator) >> 31;
    uint32_t quotient = __udivsi3(
        (uint32_t)(numerator < 0 ? -numerator : numerator),
        (uint32_t)(denominator < 0 ? -denominator : denominator));
    return negative ? -(int32_t)quotient : (int32_t)quotient;
}

int32_t __modsi3(int32_t numerator, int32_t denominator)
{
    if (denominator == 0) {
        return numerator;
    }
    if (denominator == -1) {
        return 0;
    }
    uint32_t remainder = __umodsi3(
        (uint32_t)(numerator < 0 ? -numerator : numerator),
        (uint32_t)(denominator < 0 ? -denominator : denominator));
    return numerator < 0 ? -(int32_t)remainder : (int32_t)remainder;
}
