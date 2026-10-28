/* Minimal freestanding math.h for the Setun-Doom RV32IM port (soft-float, single precision). */
#ifndef SETUN_MATH_H
#define SETUN_MATH_H

float sin(float x);
float cos(float x);
float tan(float x);
float atan(float x);
float atan2(float y, float x);
float sqrt(float x);
float fabs(float x);
float floor(float x);
float ceil(float x);
float pow(float base, float exponent);
float fmod(float x, float y);

#define M_PI 3.14159265358979323846f

#endif
