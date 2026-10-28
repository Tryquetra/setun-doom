// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT
//
// Independent hook counters for the Setun-Doom ternary bridge.
//
// The C engine increments these at each call site of a `setun_*` hook, while the Rust bridge
// keeps its own counters. Comparing both proves the hooks are actually invoked, instead of
// inferring it from the emulator telemetry.

#ifndef SETUN_HOOKS_H
#define SETUN_HOOKS_H

#include <stdint.h>

typedef struct {
    uint64_t draw_column_calls;
    uint64_t draw_column_pixels;
    uint64_t draw_span_calls;
    uint64_t draw_span_pixels;
    uint64_t fixed_mul_calls;
    uint64_t fixed_div_calls;
    uint64_t point_on_side_calls;
    uint64_t aprox_distance_calls;
} setun_hook_counters_t;

extern uint64_t setun_c_draw_column_calls;
extern uint64_t setun_c_draw_column_pixels;
extern uint64_t setun_c_draw_span_calls;
extern uint64_t setun_c_draw_span_pixels;
extern uint64_t setun_c_fixed_mul_calls;
extern uint64_t setun_c_fixed_div_calls;
extern uint64_t setun_c_point_on_side_calls;
extern uint64_t setun_c_aprox_distance_calls;

void setun_GetHookCounters(setun_hook_counters_t *out);

#endif
