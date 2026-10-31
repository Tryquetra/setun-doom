/*
 * Level 2 recording demo: authentic Doom fixed-point, bounding-box and random
 * routines compiled to LLVM IR and executed on the 21-trit Setun CPU.
 */

#include "minilibc.h"
#include "doomtype.h"
#include "m_fixed.h"
#include "m_bbox.h"
#include "m_random.h"

int main(void)
{
    int acc = 0;

    for (int iter = 0; iter < 2000; iter++) {
        fixed_t one = 1 << 16;
        fixed_t mul_res = FixedMul(2 * one, 3 * one);
        fixed_t div_res = FixedDiv(10 * one, 2 * one);

        fixed_t box[4];
        M_ClearBox(box);
        M_AddToBox(box, 10 * one, 20 * one);
        M_AddToBox(box, -5 * one, 30 * one);
        M_AddToBox(box, 50 * one, -15 * one);

        fixed_t top = box[BOXTOP] >> 16;
        fixed_t bottom = box[BOXBOTTOM] >> 16;
        fixed_t left = box[BOXLEFT] >> 16;
        fixed_t right = box[BOXRIGHT] >> 16;

        int bbox_sum = (int)(top + bottom + left + right);
        int mul_int = (int)(mul_res >> 16);
        int div_int = (int)(div_res >> 16);

        M_ClearRandom();
        int r1 = M_Random();
        int r2 = M_Random();
        int pr1 = P_Random();
        int r_sum = r1 + r2 + pr1;

        acc += mul_int + div_int + bbox_sum + r_sum + (iter & 7);
    }

    return acc;
}
