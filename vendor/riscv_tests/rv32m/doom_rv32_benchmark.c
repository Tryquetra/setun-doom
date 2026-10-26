#include <stdint.h>

typedef int32_t fixed_t;
#define FRACBITS 16
#define FRACUNIT (1 << FRACBITS)

__attribute__((noinline))
fixed_t FixedMul(fixed_t a, fixed_t b) {
    fixed_t hi, lo;
    asm volatile("mulh %0, %1, %2" : "=r"(hi) : "r"(a), "r"(b));
    asm volatile("mul  %0, %1, %2" : "=r"(lo) : "r"(a), "r"(b));
    return (hi << 16) | ((uint32_t)lo >> 16);
}

__attribute__((noinline))
fixed_t FixedDiv(fixed_t a, fixed_t b) {
    if (b == 0) return 0x7FFFFFFF;
    fixed_t q = a / b;
    fixed_t r = a % b;
    return (q << 16) + ((r << 16) / b);
}

typedef struct {
    fixed_t x, y, dx, dy;
} bsp_node_t;

__attribute__((noinline))
int R_PointOnSide(fixed_t x, fixed_t y, const bsp_node_t *node) {
    if (!node->dx) {
        if (x <= node->x)
            return node->dy > 0;
        return node->dy < 0;
    }
    if (!node->dy) {
        if (y <= node->y)
            return node->dx < 0;
        return node->dx > 0;
    }

    fixed_t dx = (x - node->x);
    fixed_t dy = (y - node->y);

    fixed_t left = FixedMul(node->dy >> FRACBITS, dx);
    fixed_t right = FixedMul(dy, node->dx >> FRACBITS);

    if (right < left) return 0; // Front
    return 1; // Back
}

#define FB_WIDTH 320
#define FB_HEIGHT 200
static uint8_t simulated_fb[FB_WIDTH * FB_HEIGHT];

__attribute__((noinline))
void R_DrawColumn(int x, int y1, int y2, fixed_t frac, fixed_t fracstep, const uint8_t *tex) {
    if (x < 0 || x >= FB_WIDTH) return;
    for (int y = y1; y <= y2 && y < FB_HEIGHT; y++) {
        int tex_y = (frac >> FRACBITS) & 0x7F;
        simulated_fb[y * FB_WIDTH + x] = tex[tex_y];
        frac += fracstep;
    }
}

static void sys_write_str(const char *s) {
    uint32_t len = 0;
    while (s[len]) len++;
    register int32_t a0 asm("a0") = 1;
    register const char *a1 asm("a1") = s;
    register uint32_t a2 asm("a2") = len;
    register int32_t a7 asm("a7") = 64;
    asm volatile("ecall" : "+r"(a0) : "r"(a1), "r"(a2), "r"(a7) : "memory");
}

int main(void) {
    sys_write_str("Iniciando Renderizador Doom RV32IM no SetunDbtEngine...\n");

    bsp_node_t partition;
    partition.x = 0;
    partition.y = 0;
    partition.dx = 0;
    partition.dy = FRACUNIT;

    int side_right = R_PointOnSide(FRACUNIT * 10, 0, &partition);
    int side_left = R_PointOnSide(-FRACUNIT * 10, 0, &partition);

    if (side_right != 0 || side_left != 1) {
        sys_write_str("Falha no R_PointOnSide BSP!\n");
        return 1;
    }

    uint8_t texture[128];
    for (int i = 0; i < 128; i++) {
        texture[i] = (uint8_t)(i ^ 0x55);
    }

    fixed_t initial_scale = FRACUNIT / 2;
    fixed_t fracstep = FixedDiv(FRACUNIT, initial_scale);

    for (int col = 0; col < FB_WIDTH; col++) {
        int y1 = 20 + (col % 10);
        int y2 = 180 - (col % 10);
        R_DrawColumn(col, y1, y2, 0, fracstep, texture);
    }

    uint32_t checksum = 0;
    for (int p = 0; p < FB_WIDTH * FB_HEIGHT; p++) {
        checksum = (checksum * 33) + simulated_fb[p];
    }

    if (checksum == 0) {
        sys_write_str("Erro: Framebuffer zerado!\n");
        return 2;
    }

    sys_write_str("Renderizacao Doom RV32IM Concluida com Sucesso!\n");
    return 0;
}
