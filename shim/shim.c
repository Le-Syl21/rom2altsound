// Minimal C glue for things that need PinMAME's internal structures (RunningMachine,
// the sound board interface table). Everything else is called directly from Rust.
// This file only reads PinMAME's headers; it never modifies PinMAME.

#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include "driver.h"
#include "cpuexec.h"
#include "wpc/sndbrd.h"
#include "wpc/core.h"

// Rebuild the board interface table exactly as src/wpc/sndbrd.c does (same X-macro list,
// index = board type >> 8), so that we can look at a board's manual-command handler.
#define SNDBRD_RECURSIVE
#define SNDBRDINTF(name) extern const struct sndbrdIntf name##Intf;
#include "wpc/sndbrd.c"
#undef SNDBRDINTF
#define SNDBRDINTF(name) &name##Intf,
static const struct sndbrdIntf *const shim_boards[] = { NULL,
#include "wpc/sndbrd.c"
};
#undef SNDBRDINTF

static const struct sndbrdIntf *board_intf(int board) {
  const int idx = sndbrd_type(board) >> 8;
  if (idx <= 0 || idx >= (int)(sizeof(shim_boards) / sizeof(shim_boards[0])))
    return NULL;
  return shim_boards[idx];
}

// 1 if the board has a manual-command handler (what sndbrd_manCmd() calls), else 0.
int shim_board_has_mancmd(int board) {
  const struct sndbrdIntf *b = board_intf(board);
  return b && b->manCmd_w;
}

// The board's SNDBRD_* flags (e.g. SNDBRD_DOUBLECMD), or 0.
unsigned shim_board_flags(int board) {
  const struct sndbrdIntf *b = board_intf(board);
  return b ? b->flags : 0;
}

// Halt (or release) every game CPU: a CPU with a type and no flags, exactly the
// selection made by PinMAME's sound commander (src/wpc/snd_cmd.c). Audio CPUs keep running.
// Returns the number of CPUs affected.
int shim_halt_game_cpus(int halt) {
  int ii, n = 0;
  for (ii = 0; ii < MAX_CPU; ii++)
    if (Machine->drv->cpu[ii].cpu_type && Machine->drv->cpu[ii].cpu_flags == 0) {
      cpunum_set_halt_line(ii, halt ? ASSERT_LINE : CLEAR_LINE);
      n++;
    }
  return n;
}

// Pulse the reset line of every audio CPU. Used to silence boards that have no
// known "stop" command. Returns the number of CPUs reset.
int shim_reset_audio_cpus(void) {
  int ii, n = 0;
  for (ii = 0; ii < MAX_CPU; ii++)
    if (Machine->drv->cpu[ii].cpu_type && (Machine->drv->cpu[ii].cpu_flags & CPU_AUDIO_CPU)) {
      cpunum_set_reset_line(ii, PULSE_LINE);
      n++;
    }
  return n;
}

// libpinmame log callback. Kept in C because it receives a va_list.
int shim_log_min_level = 2; // PINMAME_LOG_LEVEL_ERROR
void shim_log(int level, const char *format, va_list args, void *user) {
  (void)user;
  if (level < shim_log_min_level)
    return;
  fputs("[pinmame] ", stderr);
  vfprintf(stderr, format, args);
  fputc('\n', stderr);
}

// The first sound ROM region (REGION_SOUND1): on DCS boards, U2 is loaded at offset 0
// (wmssnd.h DCS_ROMREGION). Returns NULL if the machine has no such region.
const unsigned char *shim_sound_region(unsigned *len) {
  *len = (unsigned)memory_region_length(REGION_SOUND1);
  return memory_region(REGION_SOUND1);
}

// The running game's hardware generation (core_gameData->gen, GEN_* in wpc/gen.h), or 0.
// Tells Data East from Sega/Stern Whitestar, which share the "BSMT" sound board.
unsigned long long shim_game_gen(void) {
  return core_gameData ? (unsigned long long)core_gameData->gen : 0;
}

// 1 if the machine has a BSMT2000 sound chip (Data East, Sega/Stern Whitestar, Alvin G.).
int shim_has_bsmt2000(void) {
  int ii;
  for (ii = 0; ii < MAX_SOUND; ii++)
    if (Machine->drv->sound[ii].sound_type == SOUND_BSMT2000)
      return 1;
  return 0;
}

// ---------------------------------------------------------------------------------------
// Stern SAM: the PCM1755 DAC's register writes (master volume).
//
// The SAM CPU bit-bangs 16-bit words to its PCM1755 DAC on PIO lines P3-P5, which PinMAME
// sees as writes to the CPU's only I/O port (sam.c, sam_port_w): registers 0x10/0x11 are
// the left/right attenuation, 0x12 the soft mute, 0x13 the DAC off. sam.c keeps the
// result in a private struct, so the hook below sits in front of its port handler: it
// decodes the same serial words the same way, logs them with the emulated time, then calls
// sam.c's own handler, which still does all the work. Nothing in PinMAME is changed.

#define SHIM_SAM_DAC_LOG 1024
static port_write32_handler shim_sam_port_w_orig;
static int shim_sam_pass = 16;
static unsigned shim_sam_value;
static volatile int shim_sam_dac_n;
static struct { double at; unsigned char reg, val; } shim_sam_dac_log[SHIM_SAM_DAC_LOG];

static WRITE32_HANDLER(shim_sam_port_w) {
  // Same decoding as sam.c's sam_port_w: bit 4 clocks bit 5 in, MSB first; bit 3 ends.
  if ((data & 0x10) && shim_sam_pass >= 0)
    shim_sam_value |= ((data & 0x20) >> 5) << shim_sam_pass--;
  if (data & 0x08) {
    if (shim_sam_dac_n < SHIM_SAM_DAC_LOG) {
      shim_sam_dac_log[shim_sam_dac_n].at = timer_get_time();
      shim_sam_dac_log[shim_sam_dac_n].reg = (unsigned char)(shim_sam_value >> 8);
      shim_sam_dac_log[shim_sam_dac_n].val = (unsigned char)shim_sam_value;
      shim_sam_dac_n++;
    }
    shim_sam_pass = 16;
    shim_sam_value = 0;
  }
  shim_sam_port_w_orig(offset, data, mem_mask);
}

// Puts the hook in front of a SAM machine's port handler. Call it from the emulation
// thread once the machine exists and before its CPU runs (the audio-available callback).
// Returns 1 when hooked, 0 when the machine is not a SAM or its port map is not the one
// sam.c declares (one handler for ports 0x00-0xFF).
int shim_sam_hook_dac(void) {
  const struct IO_WritePort32 *p;
  if (!core_gameData || !(core_gameData->gen & GEN_SAM))
    return 0;
  p = (const struct IO_WritePort32 *)Machine->drv->cpu[0].port_write;
  if (!p)
    return 0;
  for (; !IS_MEMPORT_END(p); p++) {
    if (IS_MEMPORT_MARKER(p))
      continue;
    if (p->start == 0x00 && p->end == 0xFF && p->handler) {
      shim_sam_port_w_orig = p->handler;
      shim_sam_pass = 16;
      shim_sam_value = 0;
      shim_sam_dac_n = 0;
      install_port_write32_handler(0, 0x00, 0xFF, shim_sam_port_w);
      return 1;
    }
  }
  return 0;
}

// How many DAC writes were logged (at most SHIM_SAM_DAC_LOG).
int shim_sam_dac_count(void) {
  return shim_sam_dac_n;
}

// The i-th logged write: emulated time (s), register, value. Returns 0 when out of range.
int shim_sam_dac_get(int i, double *at, unsigned char *reg, unsigned char *val) {
  if (i < 0 || i >= shim_sam_dac_n)
    return 0;
  *at = shim_sam_dac_log[i].at;
  *reg = shim_sam_dac_log[i].reg;
  *val = shim_sam_dac_log[i].val;
  return 1;
}

// ---------------------------------------------------------------------------------------
// Sound CPU state, for finding where a music loops (the sequencer's state repeats).
//
// The state of the audio CPUs is their registers and their RAM: the RAM ranges are the
// entries of the CPU's own read map whose handler is the static RAM handler (MRA_RAM), so
// that reading them has no side effect (no latch, no I/O register). 8-bit data buses only
// (the sound CPUs of every board but DCS).

// The i-th audio CPU (CPU_AUDIO_CPU flag) with an 8-bit data bus, or -1.
int shim_audio_cpu(int i) {
  int ii;
  for (ii = 0; ii < MAX_CPU; ii++)
    if (Machine->drv->cpu[ii].cpu_type && (Machine->drv->cpu[ii].cpu_flags & CPU_AUDIO_CPU)
        && cpunum_databus_width(ii) == 8 && i-- == 0)
      return ii;
  return -1;
}

// The RAM ranges of a CPU's read map, as inclusive [start, end] pairs. Returns how many
// there are (only the first `max` are stored).
int shim_cpu_ram_ranges(int cpu, unsigned *start, unsigned *end, int max) {
  const struct Memory_ReadAddress *p;
  int n = 0;
  if (cpu < 0 || cpu >= MAX_CPU)
    return 0;
  p = (const struct Memory_ReadAddress *)Machine->drv->cpu[cpu].memory_read;
  if (!p)
    return 0;
  for (; !IS_MEMPORT_END(p); p++) {
    if (IS_MEMPORT_MARKER(p) || p->handler != MRA_RAM)
      continue;
    if (n < max) {
      start[n] = p->start;
      end[n] = p->end;
    }
    n++;
  }
  return n;
}

// Reads `len` bytes of a CPU's RAM from `addr` (RAM ranges only, see above). Static RAM
// lives in the CPU's own memory region at its address (memory.c: `rambase` is
// `memory_region(REGION_CPU1 + cpunum)`, and MRA_RAM reads `cpu_bankbase[STATIC_RAM][address]`).
// Returns 0 when the range is outside the region (nothing is read).
int shim_cpu_read(int cpu, unsigned addr, unsigned len, unsigned char *out) {
  const unsigned char *base = memory_region(REGION_CPU1 + cpu);
  if (!base || (size_t)addr + len > memory_region_length(REGION_CPU1 + cpu))
    return 0;
  memcpy(out, base + addr, len);
  return 1;
}

// A CPU register (the CPU core's own numbering, 1 = PC on the 6800/6809 families).
unsigned shim_cpu_reg(int cpu, int reg) {
  return cpunum_get_reg(cpu, reg);
}
