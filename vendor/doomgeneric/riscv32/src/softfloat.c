// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Soft-float runtime for the RISC-V guest.
//!
//! The DBT implements RV32IM without the F extension, so clang emits calls to these builtins.
//! Only single precision arithmetic is used by Doom; the double conversions exist because C
//! promotes `float` to `double` at vararg boundaries (`printf("%f", ...)`).

#include <stdint.h>

typedef union {
    float value;
    uint32_t bits;
} float_bits_t;

typedef union {
    double value;
    uint64_t bits;
} double_bits_t;

#define FLOAT_SIGN 0x80000000u
#define FLOAT_EXP_MASK 0x7F800000u
#define FLOAT_MANT_MASK 0x007FFFFFu
#define FLOAT_HIDDEN 0x00800000u
#define FLOAT_QUIET_NAN 0x7FC00000u

static float float_from_bits(uint32_t bits)
{
    float_bits_t v;
    v.bits = bits;
    return v.value;
}

static uint32_t float_to_bits(float value)
{
    float_bits_t v;
    v.value = value;
    return v.bits;
}

static int float_is_nan(uint32_t bits)
{
    return (bits & FLOAT_EXP_MASK) == FLOAT_EXP_MASK && (bits & FLOAT_MANT_MASK) != 0;
}

static int float_is_inf(uint32_t bits)
{
    return (bits & 0x7FFFFFFFu) == FLOAT_EXP_MASK;
}

static int float_is_zero(uint32_t bits)
{
    return (bits & 0x7FFFFFFFu) == 0;
}

/* Rounds a 24-bit-or-more mantissa held in `value` (implicit bit included) and packs it. */
static float float_pack(int sign, int exponent, uint32_t mantissa)
{
    if (mantissa == 0) {
        return float_from_bits((uint32_t)sign << 31);
    }
    while (mantissa & 0xFF000000u) {
        uint32_t lost = mantissa & 1u;
        mantissa >>= 1;
        if (lost) {
            mantissa |= 0x800000u;
        }
        exponent++;
    }
    while ((mantissa & FLOAT_HIDDEN) == 0) {
        mantissa <<= 1;
        exponent--;
    }
    if (exponent >= 255) {
        return float_from_bits(((uint32_t)sign << 31) | FLOAT_EXP_MASK);
    }
    if (exponent <= 0) {
        /* Subnormal or underflow: shift the implicit bit out. */
        int shift = 1 - exponent;
        if (shift > 24) {
            return float_from_bits((uint32_t)sign << 31);
        }
        mantissa >>= shift;
        return float_from_bits(((uint32_t)sign << 31) | (mantissa & FLOAT_MANT_MASK));
    }
    return float_from_bits(((uint32_t)sign << 31) | ((uint32_t)exponent << 23) |
                           (mantissa & FLOAT_MANT_MASK));
}

static uint32_t float_mantissa(uint32_t bits)
{
    uint32_t mantissa = bits & FLOAT_MANT_MASK;
    if ((bits & FLOAT_EXP_MASK) != 0) {
        mantissa |= FLOAT_HIDDEN;
    }
    return mantissa;
}

static int float_exponent(uint32_t bits)
{
    int exponent = (int)((bits & FLOAT_EXP_MASK) >> 23);
    if (exponent == 0) {
        return 1;
    }
    return exponent;
}

float __addsf3(float a, float b)
{
    uint32_t ua = float_to_bits(a);
    uint32_t ub = float_to_bits(b);
    if (float_is_nan(ua) || float_is_nan(ub)) {
        return float_from_bits(FLOAT_QUIET_NAN);
    }
    if (float_is_inf(ua) || float_is_inf(ub)) {
        if (float_is_inf(ua) && float_is_inf(ub) && ((ua ^ ub) & FLOAT_SIGN)) {
            return float_from_bits(FLOAT_QUIET_NAN);
        }
        return float_is_inf(ua) ? a : b;
    }
    if (float_is_zero(ua)) {
        return b;
    }
    if (float_is_zero(ub)) {
        return a;
    }

    int sign_a = (ua & FLOAT_SIGN) != 0;
    int sign_b = (ub & FLOAT_SIGN) != 0;
    int exp_a = float_exponent(ua);
    int exp_b = float_exponent(ub);
    uint32_t mant_a = float_mantissa(ua);
    uint32_t mant_b = float_mantissa(ub);

    if (exp_a < exp_b) {
        int swap_sign = sign_a;
        sign_a = sign_b;
        sign_b = swap_sign;
        int swap_exp = exp_a;
        exp_a = exp_b;
        exp_b = swap_exp;
        uint32_t swap_mant = mant_a;
        mant_a = mant_b;
        mant_b = swap_mant;
    }

    int shift = exp_a - exp_b;
    uint32_t sticky = 0;
    if (shift > 0) {
        if (shift > 27) {
            sticky = mant_b != 0;
            mant_b = 0;
        } else {
            sticky = (mant_b & ((1u << shift) - 1u)) != 0;
            mant_b >>= shift;
        }
    }

    uint32_t result_mant;
    int result_sign;
    int result_exp = exp_a;

    if (sign_a == sign_b) {
        result_mant = mant_a + mant_b;
        result_sign = sign_a;
        if (result_mant & 0x01000000u) {
            sticky |= result_mant & 1u;
            result_mant = (result_mant >> 1) | (result_mant & 1u);
            result_exp++;
        }
    } else {
        if (mant_a < mant_b || (mant_a == mant_b && !sticky)) {
            result_mant = mant_b - mant_a;
            result_sign = sign_b;
        } else {
            result_mant = mant_a - mant_b;
            result_sign = sign_a;
        }
        if (result_mant == 0) {
            return float_from_bits(0);
        }
    }

    if (sticky) {
        result_mant |= 1u;
    }
    return float_pack(result_sign, result_exp, result_mant);
}

float __subsf3(float a, float b)
{
    return __addsf3(a, -b);
}

float __mulsf3(float a, float b)
{
    uint32_t ua = float_to_bits(a);
    uint32_t ub = float_to_bits(b);
    if (float_is_nan(ua) || float_is_nan(ub)) {
        return float_from_bits(FLOAT_QUIET_NAN);
    }
    int sign = ((ua ^ ub) & FLOAT_SIGN) != 0;
    if (float_is_inf(ua) || float_is_inf(ub)) {
        if (float_is_zero(ua) || float_is_zero(ub)) {
            return float_from_bits(FLOAT_QUIET_NAN);
        }
        return float_from_bits(((uint32_t)sign << 31) | FLOAT_EXP_MASK);
    }
    if (float_is_zero(ua) || float_is_zero(ub)) {
        return float_from_bits((uint32_t)sign << 31);
    }

    int exp = float_exponent(ua) + float_exponent(ub) - 127;
    uint64_t product = (uint64_t)float_mantissa(ua) * (uint64_t)float_mantissa(ub);
    /* Product has 48 bits; normalize to 24 with rounding. */
    int shift = 0;
    while (product & ~0xFFFFFFull) {
        product >>= 1;
        shift++;
    }
    uint32_t mantissa = (uint32_t)product;
    uint32_t round = (uint32_t)(product & 0xFFFFFFFFu);
    if (round) {
        mantissa |= 1u;
    }
    return float_pack(sign, exp + shift, mantissa);
}

float __divsf3(float a, float b)
{
    uint32_t ua = float_to_bits(a);
    uint32_t ub = float_to_bits(b);
    if (float_is_nan(ua) || float_is_nan(ub)) {
        return float_from_bits(FLOAT_QUIET_NAN);
    }
    int sign = ((ua ^ ub) & FLOAT_SIGN) != 0;
    if (float_is_inf(ua)) {
        if (float_is_inf(ub)) {
            return float_from_bits(FLOAT_QUIET_NAN);
        }
        return float_from_bits(((uint32_t)sign << 31) | FLOAT_EXP_MASK);
    }
    if (float_is_inf(ub)) {
        return float_from_bits((uint32_t)sign << 31);
    }
    if (float_is_zero(ub)) {
        if (float_is_zero(ua)) {
            return float_from_bits(FLOAT_QUIET_NAN);
        }
        return float_from_bits(((uint32_t)sign << 31) | FLOAT_EXP_MASK);
    }
    if (float_is_zero(ua)) {
        return float_from_bits((uint32_t)sign << 31);
    }

    int exp = float_exponent(ua) - float_exponent(ub) + 127;
    uint64_t numerator = (uint64_t)float_mantissa(ua) << 24;
    uint64_t quotient = numerator / (uint64_t)float_mantissa(ub);
    uint64_t remainder = numerator % (uint64_t)float_mantissa(ub);
    uint32_t mantissa = (uint32_t)quotient;
    if (remainder != 0) {
        mantissa |= 1u;
    }
    return float_pack(sign, exp - 1, mantissa);
}

int __eqsf2(float a, float b)
{
    uint32_t ua = float_to_bits(a);
    uint32_t ub = float_to_bits(b);
    if (float_is_nan(ua) || float_is_nan(ub)) {
        return 1;
    }
    return a == b ? 0 : 1;
}

int __nesf2(float a, float b)
{
    return __eqsf2(a, b);
}

int __ltsf2(float a, float b)
{
    uint32_t ua = float_to_bits(a);
    uint32_t ub = float_to_bits(b);
    if (float_is_nan(ua) || float_is_nan(ub)) {
        return 1;
    }
    if ((ua == ub) && ua != 0x80000000u) {
        return 0;
    }
    if (ua == ub) {
        return 0;
    }
    return a < b ? -1 : 1;
}

int __lesf2(float a, float b)
{
    if (__eqsf2(a, b) == 0) {
        return 0;
    }
    return __ltsf2(a, b);
}

int __gtsf2(float a, float b)
{
    return __ltsf2(b, a);
}

int __gesf2(float a, float b)
{
    return __lesf2(b, a);
}

float __floatsisf(int value)
{
    if (value == 0) {
        return 0.0f;
    }
    int sign = value < 0;
    uint32_t magnitude = sign ? (uint32_t)(-(long)value) : (uint32_t)value;
    int exponent = 127 + 31;
    while ((magnitude & 0x80000000u) == 0) {
        magnitude <<= 1;
        exponent--;
    }
    uint32_t mantissa = magnitude >> 8;
    if (magnitude & 0xFF) {
        mantissa |= 1u;
    }
    return float_pack(sign, exponent, mantissa);
}

float __floatunsisf(unsigned int value)
{
    if (value == 0) {
        return 0.0f;
    }
    int exponent = 127 + 31;
    while ((value & 0x80000000u) == 0) {
        value <<= 1;
        exponent--;
    }
    uint32_t mantissa = value >> 8;
    if (value & 0xFF) {
        mantissa |= 1u;
    }
    return float_pack(0, exponent, mantissa);
}

int __fixsfsi(float value)
{
    uint32_t bits = float_to_bits(value);
    if (float_is_nan(bits) || float_is_inf(bits)) {
        return (bits & FLOAT_SIGN) ? (int)0x80000000 : (int)0x7FFFFFFF;
    }
    int exponent = float_exponent(bits) - 127;
    if (exponent < 0) {
        return 0;
    }
    uint32_t mantissa = float_mantissa(bits);
    if (exponent > 30) {
        return (bits & FLOAT_SIGN) ? (int)0x80000000 : (int)0x7FFFFFFF;
    }
    uint32_t magnitude = exponent >= 23 ? (mantissa << (exponent - 23))
                                        : (mantissa >> (23 - exponent));
    return (bits & FLOAT_SIGN) ? -(int)magnitude : (int)magnitude;
}

unsigned int __fixunssfsi(float value)
{
    int converted = __fixsfsi(value);
    if (converted < 0) {
        return 0;
    }
    return (unsigned int)converted;
}

double __extendsfdf2(float value)
{
    uint32_t bits = float_to_bits(value);
    uint64_t sign = (uint64_t)(bits & FLOAT_SIGN) << 32;
    uint32_t exponent = (bits & FLOAT_EXP_MASK) >> 23;
    uint32_t mantissa = bits & FLOAT_MANT_MASK;
    double_bits_t result;
    if (exponent == 0xFF) {
        result.bits = sign | 0x7FF0000000000000ull | ((uint64_t)mantissa << 29);
        return result.value;
    }
    if (exponent == 0 && mantissa == 0) {
        result.bits = sign;
        return result.value;
    }
    if (exponent == 0) {
        /* Normalize the subnormal float into a normal double. */
        int shift = 0;
        while ((mantissa & FLOAT_HIDDEN) == 0) {
            mantissa <<= 1;
            shift++;
        }
        mantissa &= FLOAT_MANT_MASK;
        exponent = 1 - shift;
    }
    result.bits = sign | ((uint64_t)(exponent + 896) << 52) | ((uint64_t)mantissa << 29);
    return result.value;
}

float __truncdfsf2(double value)
{
    double_bits_t input;
    input.value = value;
    uint32_t sign = (uint32_t)((input.bits >> 32) & FLOAT_SIGN);
    int exponent = (int)((input.bits >> 52) & 0x7FF);
    uint64_t mantissa = input.bits & 0x000FFFFFFFFFFFFFull;
    if (exponent == 0x7FF) {
        return float_from_bits(sign | FLOAT_EXP_MASK | (mantissa ? 0x400000u : 0));
    }
    if (exponent == 0) {
        return float_from_bits(sign);
    }
    int unbiased = exponent - 1023;
    if (unbiased > 127) {
        return float_from_bits(sign | FLOAT_EXP_MASK);
    }
    if (unbiased < -126) {
        return float_from_bits(sign);
    }
    uint32_t narrowed = (uint32_t)(mantissa >> 29);
    if (mantissa & 0x1FFFFFFFull) {
        narrowed |= 1u;
    }
    return float_pack((sign != 0), unbiased + 127, narrowed | FLOAT_HIDDEN);
}

/* 64-bit unsigned division used by the float divider (RV32 has no DIVU64). */
unsigned long long __udivdi3(unsigned long long numerator, unsigned long long denominator)
{
    if (denominator == 0) {
        return 0;
    }
    unsigned long long quotient = 0;
    unsigned long long remainder = 0;
    for (int bit = 63; bit >= 0; bit--) {
        remainder = (remainder << 1) | ((numerator >> bit) & 1ull);
        if (remainder >= denominator) {
            remainder -= denominator;
            quotient |= 1ull << bit;
        }
    }
    return quotient;
}

unsigned long long __umoddi3(unsigned long long numerator, unsigned long long denominator)
{
    if (denominator == 0) {
        return 0;
    }
    unsigned long long remainder = 0;
    for (int bit = 63; bit >= 0; bit--) {
        remainder = (remainder << 1) | ((numerator >> bit) & 1ull);
        if (remainder >= denominator) {
            remainder -= denominator;
        }
    }
    return remainder;
}

int __unordsf2(float a, float b)
{
    return float_is_nan(float_to_bits(a)) || float_is_nan(float_to_bits(b));
}

int __ltdf2(double a, double b)
{
    double_bits_t ua;
    double_bits_t ub;
    ua.value = a;
    ub.value = b;
    uint64_t magnitude_a = ua.bits & 0x7FFFFFFFFFFFFFFFull;
    uint64_t magnitude_b = ub.bits & 0x7FFFFFFFFFFFFFFFull;
    int negative_a = (ua.bits >> 63) != 0;
    int negative_b = (ub.bits >> 63) != 0;
    if (magnitude_a > 0x7FF0000000000000ull || magnitude_b > 0x7FF0000000000000ull) {
        return 1;
    }
    if (negative_a != negative_b) {
        return negative_a ? -1 : 1;
    }
    if (negative_a) {
        return magnitude_a > magnitude_b ? -1 : (magnitude_a == magnitude_b ? 0 : 1);
    }
    return magnitude_a < magnitude_b ? -1 : (magnitude_a == magnitude_b ? 0 : 1);
}

/* 64-bit integer helpers used by the 64-bit `FixedMul`/`FixedDiv` in `m_fixed.c`. */
long long __muldi3(long long a, long long b)
{
    /* Shift-add: a 64-bit `*` would lower to a recursive call to this helper on rv32i. */
    unsigned long long ua = (unsigned long long)a;
    unsigned long long ub = (unsigned long long)b;
    unsigned long long result = 0;

    while (ub != 0) {
        if (ub & 1ull) {
            result += ua;
        }
        ua <<= 1;
        ub >>= 1;
    }
    return (long long)result;
}

static unsigned long long udivmod64(unsigned long long numerator, unsigned long long denominator,
                                    unsigned long long *remainder)
{
    unsigned long long quotient = 0;
    unsigned long long rest = 0;
    for (int bit = 63; bit >= 0; bit--) {
        rest = (rest << 1) | ((numerator >> bit) & 1ull);
        if (rest >= denominator) {
            rest -= denominator;
            quotient |= 1ull << bit;
        }
    }
    if (remainder != 0) {
        *remainder = rest;
    }
    return quotient;
}

long long __divdi3(long long a, long long b)
{
    int negative = (a < 0) != (b < 0);
    unsigned long long magnitude_a = a < 0 ? (unsigned long long)(-a) : (unsigned long long)a;
    unsigned long long magnitude_b = b < 0 ? (unsigned long long)(-b) : (unsigned long long)b;
    unsigned long long quotient = udivmod64(magnitude_a, magnitude_b, 0);
    return negative ? -(long long)quotient : (long long)quotient;
}

long long __moddi3(long long a, long long b)
{
    unsigned long long magnitude_a = a < 0 ? (unsigned long long)(-a) : (unsigned long long)a;
    unsigned long long magnitude_b = b < 0 ? (unsigned long long)(-b) : (unsigned long long)b;
    unsigned long long remainder = 0;
    udivmod64(magnitude_a, magnitude_b, &remainder);
    return a < 0 ? -(long long)remainder : (long long)remainder;
}
