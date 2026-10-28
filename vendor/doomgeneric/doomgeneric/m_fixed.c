//
// Copyright(C) 1993-1996 Id Software, Inc.
// Copyright(C) 2005-2014 Simon Howard
// Modified on 2026-10-25 by Robson Cassiano (https://eu.robsoncassiano.software)
// to route FixedMul and FixedDiv through the Setun-Doom 21-trit balanced ternary ALU.
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// DESCRIPTION:
//	Fixed point implementation.
//



#include "stdlib.h"

#include "doomtype.h"
#include "i_system.h"

#include "m_fixed.h"
#include "setun_hooks.h"

#ifdef SETUN_TERNARY_ALU
extern int32_t setun_FixedMul(int32_t a, int32_t b);
extern int32_t setun_FixedDiv(int32_t a, int32_t b);

fixed_t FixedMul(fixed_t a, fixed_t b)
{
    setun_c_fixed_mul_calls++;
    return (fixed_t)setun_FixedMul((int32_t)a, (int32_t)b);
}

fixed_t FixedDiv(fixed_t a, fixed_t b)
{
    setun_c_fixed_div_calls++;
    return (fixed_t)setun_FixedDiv((int32_t)a, (int32_t)b);
}
#else
// Fixme. __USE_C_FIXED__ or something.

#ifdef SETUN_RV32_GUEST
fixed_t
FixedMul
( fixed_t	a,
  fixed_t	b )
{
    // 64-bit product: the 32-bit split algorithm overflows for large operands and was the
    // source of diverging projection math on the RV32 guest.
    return (fixed_t)(((int64_t)a * (int64_t)b) >> FRACBITS);
}
#else
fixed_t
FixedMul
( fixed_t	a,
  fixed_t	b )
{
    int ahi = a >> FRACBITS;
    int alo = a & ((1 << FRACBITS) - 1);
    return (ahi * b) + ((alo * b) >> FRACBITS);
}
#endif

//
// FixedDiv, C version.
//

#ifdef SETUN_RV32_GUEST
// Computes trunc((remainder * 2^16) / divisor) with 32-bit arithmetic only.
// `|remainder| < |divisor|`, so the quotient fits 16 bits and the RV32 guest avoids the
// 64-bit division runtime on every projection call.
static fixed_t FixedDivFraction(int remainder, int divisor)
{
    unsigned int magnitude = remainder < 0 ? (unsigned int)(-remainder)
                                           : (unsigned int)remainder;
    unsigned int denominator = divisor < 0 ? (unsigned int)(-divisor)
                                           : (unsigned int)divisor;
    unsigned int quotient = 0;

    for (int bit = 0; bit < 16; bit++)
    {
        quotient <<= 1;
        magnitude <<= 1;
        if (magnitude >= denominator)
        {
            magnitude -= denominator;
            quotient |= 1;
        }
    }

    return (fixed_t)((remainder < 0) != (divisor < 0) ? -(int)quotient : (int)quotient);
}

fixed_t FixedDiv(fixed_t a, fixed_t b)
{
    if ((abs(a) >> 14) >= abs(b))
    {
	return (a^b) < 0 ? INT_MIN : INT_MAX;
    }
    else
    {
	int q = a / b;
	int r = a % b;
	return (q << FRACBITS) + FixedDivFraction(r, b);
    }
}
#else
fixed_t FixedDiv(fixed_t a, fixed_t b)
{
    if ((abs(a) >> 14) >= abs(b))
    {
	return (a^b) < 0 ? INT_MIN : INT_MAX;
    }
    else
    {
	int q = a / b;
	int r = a % b;
	return (q << FRACBITS) + ((r << FRACBITS) / b);
    }
}
#endif


#endif /* SETUN_TERNARY_ALU */
