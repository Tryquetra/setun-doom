// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

// Arithmetic program for the Level 5 backend: it exercises add, sub, mul, div, rem, shifts,
// comparisons, branches, a loop, a call and stack slots, with no libc. The signed division and
// remainder cases check that the software helpers truncate toward zero and keep the dividend sign
// on the remainder, as C requires.

static int accumulate(int n)
{
    int total = 0;
    int value = 1;
    for (int i = 0; i < n; i = i + 1) {
        total = total + value;
        value = value * 3;
    }
    return total;
}

static int clamp_positive(int x)
{
    if (x < 0) {
        return 0;
    }
    return x;
}

int main(void)
{
    int a = 37;
    int b = 5;
    int quotient = a / b;
    int remainder = a % b;
    int product = (a + b) * (a - b);
    int shifted = product >> 2;
    int total = accumulate(4);
    int guarded = clamp_positive(0 - 7);
    int negative_quotient = (0 - 37) / 5;
    int negative_remainder = (0 - 37) % 5;
    int mixed_quotient = 37 / (0 - 5);
    int mixed_remainder = 37 % (0 - 5);
    int negative_product = (0 - 3) * 5;
    int large_quotient = 1000000 / 7;
    int large_remainder = 1000000 % 7;

    int ok = quotient == 7 && remainder == 2 && product == 1344 && shifted == 336;
    ok = ok && total == 40 && guarded == 0 && negative_quotient == 0 - 7 && negative_remainder == 0 - 2;
    ok = ok && mixed_quotient == 0 - 7 && mixed_remainder == 2 && negative_product == 0 - 15;
    ok = ok && large_quotient == 142857 && large_remainder == 1;
    if (!ok) {
        return 0 - 1;
    }

    int score = product + quotient + remainder + total;
    score = score + negative_quotient + negative_remainder + mixed_quotient + mixed_remainder;
    score = score + negative_product + large_quotient + large_remainder;
    return score;
}
