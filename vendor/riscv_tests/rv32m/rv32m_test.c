volatile int a = 12345;
volatile int b = -6789;
volatile int c = 1000;
volatile int d = 33;
volatile int zero = 0;
volatile int min_int = -2147483648;
volatile unsigned int ua = 0xFEDCBA98U;
volatile unsigned int ub = 0x76543210U;

int main(void) {
    if (a * b != -83810205) return 1;

    long long prod_s = (long long)a * (long long)b;
    int mulh_s = (int)(prod_s >> 32);

    unsigned long long prod_u = (unsigned long long)ua * (unsigned long long)ub;
    unsigned int mulhu_u = (unsigned int)(prod_u >> 32);

    int d1 = c / d;
    int r1 = c % d;
    if (d1 != 30 || r1 != 10) return 2;

    int d2 = (-c) / d;
    int r2 = (-c) % d;
    if (d2 != -30 || r2 != -10) return 3;

    if ((c / zero) != -1) return 4;
    if ((c % zero) != c) return 5;

    if ((min_int / -1) != min_int) return 6;
    if ((min_int % -1) != 0) return 7;

    unsigned int udiv = (unsigned int)c / (unsigned int)d;
    unsigned int urem = (unsigned int)c % (unsigned int)d;
    if (udiv != 30U || urem != 10U) return 8;

    return 0;
}
