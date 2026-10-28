// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Stubs for the sound, music, CD and joystick backends on bare metal. The guest runs with
//! `-nosound`, so these only need to satisfy the linker and keep the engine's checks happy.

#include <stddef.h>

#include "doomtype.h"
#include "i_sound.h"

int I_GetSfxLumpNum(sfxinfo_t *sfxinfo)
{
    (void)sfxinfo;
    return -1;
}

void I_InitSound(boolean use_sfx_prefix)
{
    (void)use_sfx_prefix;
}

void I_ShutdownSound(void)
{
}

void I_UpdateSound(void)
{
}

void I_UpdateSoundParams(int channel, int vol, int sep)
{
    (void)channel;
    (void)vol;
    (void)sep;
}

int I_StartSound(sfxinfo_t *sfxinfo, int channel, int vol, int sep)
{
    (void)sfxinfo;
    (void)channel;
    (void)vol;
    (void)sep;
    return channel;
}

void I_StopSound(int channel)
{
    (void)channel;
}

boolean I_SoundIsPlaying(int channel)
{
    (void)channel;
    return false;
}

void I_PrecacheSounds(sfxinfo_t *sounds, int num_sounds)
{
    (void)sounds;
    (void)num_sounds;
}

void I_InitMusic(void)
{
}

void I_ShutdownMusic(void)
{
}

void I_SetMusicVolume(int volume)
{
    (void)volume;
}

void I_PauseSong(void)
{
}

void I_ResumeSong(void)
{
}

void *I_RegisterSong(void *data, int len)
{
    (void)len;
    return data;
}

void I_UnRegisterSong(void *handle)
{
    (void)handle;
}

void I_PlaySong(void *handle, boolean looping)
{
    (void)handle;
    (void)looping;
}

void I_StopSong(void)
{
}

boolean I_MusicIsPlaying(void)
{
    return false;
}

void I_BindSoundVariables(void)
{
}

int cd_Error = 0;
int snd_musicdevice = 0;
int snd_sfxdevice = 0;
int snd_musicvolume = 0;
int snd_sfxvolume = 0;

int I_CDMusInit(void)
{
    return -1;
}

void I_CDMusPrintStartup(void)
{
}

int I_CDMusPlay(int track)
{
    (void)track;
    return -1;
}

int I_CDMusStop(void)
{
    return -1;
}

int I_CDMusResume(void)
{
    return -1;
}

int I_CDMusSetVolume(int volume)
{
    (void)volume;
    return -1;
}

int I_CDMusFirstTrack(void)
{
    return 0;
}

int I_CDMusLastTrack(void)
{
    return 0;
}

int I_CDMusTrackLength(int track)
{
    (void)track;
    return 0;
}

void I_InitJoystick(void)
{
}

void I_ShutdownJoystick(void)
{
}

void I_UpdateJoystick(void)
{
}

void I_BindJoystickVariables(void)
{
}
