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
#include "wpc/wmssnd.h"
#include "cpu/adsp2100/adsp2100.h"

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

// ---------------------------------------------------------------------------------------
// The sound board's type with its variant (SNDBRD_TYPE(index, subtype), wpc/sndbrd.h), or 0.
// Tells Bally's Sounds Plus -56 (SNDBRD_BY56) from the -51 (SNDBRD_BY51): PinMAME names
// both "BY51".
int shim_board_type(int board) {
  return sndbrd_type(board);
}

// ---------------------------------------------------------------------------------------
// Bally Sounds Plus -56: a command is a byte sent as two nibbles on the same four lines.
//
// The game puts the low nibble on the lines and strobes the board; the sound CPU's
// interrupt handler reads the lines through the AY-3-8910's port A, waits about 50 us,
// and reads them again, when the game has put the high nibble there (xenon's sound ROM,
// $F02E-$F078). PinMAME's manual command (by35snd.c `sp51_manCmd_w`) leaves one byte on
// the lines for both reads, so only the bytes whose two nibbles are equal reach the board.
//
// The hook below sits in front of the sound CPU's handler for the PIA that reads port A
// (as the Stern SAM hook above): once a manual command is armed, the first read of the
// PIA's port A register goes through unchanged (the low nibble), then the high nibble is
// put on the lines through the board's own data handler, for the second read. Nothing in
// PinMAME is changed.

static mem_read_handler shim_nib_orig;
static const struct sndbrdIntf *shim_nib_intf;
static int shim_nib_board;
static int shim_nib_hi = -1;  // the high nibble still to hand over, or -1
static int shim_nib_reads;    // port A reads since the last armed command

static READ_HANDLER(shim_nib_r) {
  data8_t v = shim_nib_orig(offset);
  if ((offset & 3) == 0) {
    shim_nib_reads++;
    if (shim_nib_hi >= 0) {
      shim_nib_intf->data_w(shim_nib_board, shim_nib_hi);
      shim_nib_hi = -1;
    }
  }
  return v;
}

// Puts the hook in front of the audio CPU's PIA (the read map entry at $0080) of `board`.
// Call it from the emulation thread, between two frames. Returns 1 when hooked (or already
// hooked), 0 when the board has no data handler or no audio CPU maps a PIA at $0080.
int shim_nibble_hook(int board) {
  int ii;
  const struct sndbrdIntf *b = board_intf(board);
  if (shim_nib_orig)
    return 1;
  if (!b || !b->data_w)
    return 0;
  for (ii = 0; ii < MAX_CPU; ii++) {
    const struct Memory_ReadAddress *p;
    if (!Machine->drv->cpu[ii].cpu_type || !(Machine->drv->cpu[ii].cpu_flags & CPU_AUDIO_CPU))
      continue;
    p = (const struct Memory_ReadAddress *)Machine->drv->cpu[ii].memory_read;
    for (; p && !IS_MEMPORT_END(p); p++) {
      if (IS_MEMPORT_MARKER(p) || p->start != 0x0080 || !p->handler)
        continue;
      shim_nib_orig = p->handler;
      shim_nib_intf = b;
      shim_nib_board = board;
      shim_nib_hi = -1;
      install_mem_read_handler(ii, p->start, p->end, shim_nib_r);
      return 1;
    }
  }
  return 0;
}

// Sends `data` to a hooked board as the game does: the low nibble with the strobe (the
// board's manual command), the high nibble right after the board's first read.
void shim_nibble_cmd(int board, int data) {
  shim_nib_reads = 0;
  shim_nib_hi = (data >> 4) & 0x0f;
  sndbrd_manCmd(board, data);
}

// How many times the board read its command lines since the last `shim_nibble_cmd`.
int shim_nibble_reads(void) {
  return shim_nib_reads;
}

// Writes the bytes of one multi-byte command to a board's data port the way the game CPU
// does: back to back, with only a few timeslices between them (as `wpcs_manCmd_w` does for
// its pairs), instead of one byte per frame. The board's data handler is called directly:
// `sndbrd_data_w` defers each write to a timer, so several writes in a row would overwrite
// each other in the latch before the sound CPU reads it. Returns 0 if the board has no
// data handler.
int shim_data_burst(int board, const unsigned char *bytes, int n) {
  const struct sndbrdIntf *b = board_intf(board);
  int i, j;
  if (!b || !b->data_w)
    return 0;
  for (i = 0; i < n; i++) {
    if (i > 0)
      for (j = 0; j < 12; j++) run_one_timeslice();
    b->data_w(board, bytes[i]);
  }
  return 1;
}

// ---------------------------------------------------------------------------------------
// AC-couples the machine's 8-bit DACs (dac.c), as the real boards' outputs are. PinMAME
// maps these DACs unsigned (`DAC_data_w`: code 0 = output 0, code FF = +32767), while the
// sound programs play around the middle code and leave the DAC on the last value a sound
// wrote: up to +16384 of DC in the 16-bit mix, which never reached the real speaker and
// clipped the loud sounds (taf_l5 `C7`) in emulation. dac.c already has the fix, opt-in per
// channel: a 10 Hz one-pole high-pass (`DAC_DC_offset_correction_data_16_w`, used by the
// Gottlieb, Taito and Mr. Game drivers) on the raw level that `DAC_data_w` stores, which is
// the same `data * 0x101 / 2`. One write through it switches the channel over for good
// (until the machine stops): the board's own `DAC_data_w` writes then go through the
// filter, with the same scale, and the volume stages after the DAC are untouched. The
// channel is set to 0 (its power-on level) so that the switch makes no step of its own.
// Returns how many DACs were switched. DAC_sh_start numbers the DACs of the machine's last
// DAC entry only (`n_chips = intf->num`), hence the last entry's count.
int shim_dac_ac_couple(void) {
  int ii, n = 0;
  for (ii = 0; ii < MAX_SOUND; ii++)
    if (Machine->drv->sound[ii].sound_type == SOUND_DAC)
      n = ((const struct DACinterface *)Machine->drv->sound[ii].sound_interface)->num;
  for (ii = 0; ii < n; ii++)
    DAC_DC_offset_correction_data_16_w(ii, 0);
  return n;
}

// ---------------------------------------------------------------------------------------
// The driver table: every game PinMAME knows, with its ROM list exactly as the ROM_START
// blocks declare it (name, size, hashes, region). `rom2altsound roms` identifies ROM zips
// with it, so the data always matches the emulator linked in. Nothing here needs a running
// machine, except shim_driver_init_board (see there).

#include "hash.h"

int shim_driver_count(void) {
  static int n = -1;
  if (n < 0)
    for (n = 0; drivers[n]; n++) {}
  return n;
}

// Text fields of driver i: 0 name, 1 parent's name ("" when none), 2 description, 3 year,
// 4 manufacturer, 5 source file, 6 the NOT_A_DRIVER sets up its clone_of chain, nearest
// first, separated by spaces (the shared "system" ROM sets, like gts80s: PinMAME's ROM
// loader also looks for the game's files in their zips). NULL when out of range.
const char *shim_driver_text(int i, int field) {
  static char chain[512];
  const struct GameDriver *d, *p;
  if (i < 0 || i >= shim_driver_count())
    return NULL;
  d = drivers[i];
  switch (field) {
  case 6:
    chain[0] = 0;
    for (p = d->clone_of; p; p = p->clone_of)
      if ((p->flags & NOT_A_DRIVER) && p->name && *p->name
          && strlen(chain) + strlen(p->name) + 2 < sizeof chain) {
        if (chain[0])
          strcat(chain, " ");
        strcat(chain, p->name);
      }
    return chain;
  case 0: return d->name;
  case 1: return (d->clone_of && !(d->clone_of->flags & NOT_A_DRIVER)) ? d->clone_of->name : "";
  case 2: return d->description;
  case 3: return d->year;
  case 4: return d->manufacturer;
  case 5: return d->source_file;
  }
  return NULL;
}

unsigned shim_driver_flags(int i) {
  return (i < 0 || i >= shim_driver_count()) ? 0 : drivers[i]->flags;
}

// One ROM file of a driver (a ROM_LOAD and its ROM_CONTINUE chunks).
struct shim_rom {
  const char *name;
  unsigned length;        // sum of the file's chunks: the file's size
  unsigned region;        // REGION_* (common.h)
  unsigned region_flags;  // ROMREGION_* flags
  int sound_only;         // ROMREGION_SOUNDONLY: loaded only when sound is on
  int optional, no_dump, bad_dump, bios;
  char crc[16];           // lowercase hex, "" when unknown
  char sha1[48];          // lowercase hex, "" when unknown
};

// Fills `out` with ROM j of driver i; returns 0 when there is no such ROM.
int shim_driver_rom(int i, int j, struct shim_rom *out) {
  const struct RomModule *region, *rom, *chunk;
  if (i < 0 || i >= shim_driver_count() || !drivers[i]->rom)
    return 0;
  for (region = rom_first_region(drivers[i]); region; region = rom_next_region(region))
    for (rom = rom_first_file(region); rom; rom = rom_next_file(rom)) {
      const char *h;
      if (j-- > 0)
        continue;
      memset(out, 0, sizeof(*out));
      h = ROM_GETHASHDATA(rom);
      out->name = ROM_GETNAME(rom);
      for (chunk = rom_first_chunk(rom); chunk; chunk = rom_next_chunk(chunk))
        out->length += ROM_GETLENGTH(chunk);
      out->region = ROMREGION_GETTYPE(region);
      out->region_flags = ROMREGION_GETFLAGS(region);
      out->sound_only = ROMREGION_ISSOUNDONLY(region);
      out->optional = ROM_ISOPTIONAL(rom);
      out->no_dump = hash_data_has_info(h, HASH_INFO_NO_DUMP);
      out->bad_dump = hash_data_has_info(h, HASH_INFO_BAD_DUMP);
      out->bios = ROM_GETBIOSFLAGS(rom);
      if (!hash_data_extract_printable_checksum(h, HASH_CRC, out->crc))
        out->crc[0] = 0;
      if (!hash_data_extract_printable_checksum(h, HASH_SHA1, out->sha1))
        out->sha1[0] = 0;
      return 1;
    }
  return 0;
}

// The driver's CPUs and sound chips, from its machine driver (the constructor only fills a
// structure). Returns the number of CPUs; `audio_mask` gets bit n set when CPU n is an audio
// CPU (its ROM region, REGION_CPU1 + n, then holds sound program code).
static struct InternalMachineDriver shim_mdrv;
int shim_driver_machine(int i, unsigned *audio_mask) {
  int ii, n = 0;
  *audio_mask = 0;
  if (i < 0 || i >= shim_driver_count() || !drivers[i]->drv)
    return 0;
  expand_machine_driver(drivers[i]->drv, &shim_mdrv);
  for (ii = 0; ii < MAX_CPU; ii++)
    if (shim_mdrv.cpu[ii].cpu_type) {
      n++;
      if (shim_mdrv.cpu[ii].cpu_flags & CPU_AUDIO_CPU)
        *audio_mask |= 1u << ii;
    }
  return n;
}

// After shim_driver_machine: the name of CPU k / sound chip k, or NULL.
const char *shim_machine_cpu(int k) {
  return (k >= 0 && k < MAX_CPU && shim_mdrv.cpu[k].cpu_type)
    ? cputype_name(shim_mdrv.cpu[k].cpu_type) : NULL;
}
const char *shim_machine_sound(int k) {
  return (k >= 0 && k < MAX_SOUND && shim_mdrv.sound[k].sound_type)
    ? sound_name(&shim_mdrv.sound[k]) : NULL;
}

// ---------------------------------------------------------------------------------------
// Which sound board a game has. Most drivers' machine init hands core_gameData->hw.soundBoard
// to sndbrd_0_init, but a few CPU families pick the board themselves from the generation
// (core_gameData->gen) or from their own init: WPC (wpc.c), System 11 and Data East
// alphanumeric (s11.c), System 3-7 (s4.c, s6.c, s7.c), Whitestar (se.c), Pinball 2000
// (p2k.c). shim_driver_core_init names the machine init a driver uses, compared by address
// with the init of a machine driver of each of these families; the Rust side then applies
// the family's own choice (drivers.rs, `sound_boards`).

extern void construct_wpc_alpha1S(struct InternalMachineDriver *);
extern void construct_s11_s9S(struct InternalMachineDriver *);
extern void construct_s11_s9PS(struct InternalMachineDriver *);
extern void construct_s7(struct InternalMachineDriver *);
extern void construct_s7S6(struct InternalMachineDriver *);
extern void construct_s7SND(struct InternalMachineDriver *);
extern void construct_s7RR(struct InternalMachineDriver *);
extern void construct_s6(struct InternalMachineDriver *);
extern void construct_s4(struct InternalMachineDriver *);
extern void construct_se2aS(struct InternalMachineDriver *);
extern void construct_se3aS(struct InternalMachineDriver *);
extern void construct_p2k(struct InternalMachineDriver *);

static struct {
  const char *name;
  void (*ctor)(struct InternalMachineDriver *);
  void (*init)(void);
} shim_inits[] = {
  {"wpc", construct_wpc_alpha1S}, {"s11", construct_s11_s9S}, {"s9pf", construct_s11_s9PS},
  {"s7", construct_s7}, {"s7S6", construct_s7S6}, {"s7nd", construct_s7SND},
  {"rr", construct_s7RR}, {"s6", construct_s6}, {"s4", construct_s4},
  {"se", construct_se2aS}, {"se3", construct_se3aS}, {"p2k", construct_p2k},
};

// The name of driver i's machine init when it is one of the above, else "".
const char *shim_driver_core_init(int i) {
  static int ready;
  static struct InternalMachineDriver m;
  unsigned k, mask;
  if (!ready) {
    for (k = 0; k < sizeof(shim_inits) / sizeof(shim_inits[0]); k++) {
      expand_machine_driver(shim_inits[k].ctor, &m);
      shim_inits[k].init = m.pinmame.init;
    }
    ready = 1;
  }
  if (!shim_driver_machine(i, &mask))
    return "";
  for (k = 0; k < sizeof(shim_inits) / sizeof(shim_inits[0]); k++)
    if (shim_mdrv.pinmame.init && shim_mdrv.pinmame.init == shim_inits[k].init)
      return shim_inits[k].name;
  return "";
}

// The game data of each driver from `start` on, one line each on stdout:
// "R2A <index> <ok> <hw.soundBoard> <gen> <core init or -> <ok|crash>". The game data is
// only set by the driver's init function, which may also touch the machine (install
// handlers...) and crash or exit without one: run this only in a throwaway process. Its
// first statement sets core_gameData, so a crash or an exit still reports it (marked
// "crash"), and the caller starts another process after that driver.
#include <signal.h>
static volatile int shim_cur = -1;
static const char *shim_cur_init = "-";
static void shim_board_line(const char *how) {
  if (shim_cur < 0)
    return;
  printf("R2A\t%d\t%d\t%u\t%llu\t%s\t%s\n", shim_cur, core_gameData ? 1 : 0,
         core_gameData ? (unsigned)core_gameData->hw.soundBoard : 0u,
         core_gameData ? (unsigned long long)core_gameData->gen : 0ull,
         shim_cur_init, how);
  fflush(stdout);
  shim_cur = -1;
}
static void shim_board_at_exit(void) { shim_board_line("crash"); }
static void shim_board_on_signal(int sig) { (void)sig; shim_board_line("crash"); _Exit(3); }

void shim_print_driver_boards(int start) {
  int i, n = shim_driver_count();
  atexit(shim_board_at_exit);
  signal(SIGSEGV, shim_board_on_signal);
  signal(SIGABRT, shim_board_on_signal);
  signal(SIGFPE, shim_board_on_signal);
  signal(SIGILL, shim_board_on_signal);
  for (i = start < 0 ? 0 : start; i < n; i++) {
    const char *init = shim_driver_core_init(i);
    shim_cur_init = *init ? init : "-";
    core_gameData = NULL;
    // Some inits look at the game's name (fh.c: fh_pa1 has its own game data).
    Machine->gamedrv = drivers[i];
    Machine->drv = &shim_mdrv;  // expanded by shim_driver_core_init
    shim_cur = i;
    if (drivers[i]->driver_init && !(drivers[i]->flags & NOT_A_DRIVER))
      drivers[i]->driver_init();
    shim_board_line("ok");
  }
}

// The interface name of a sound board type (SNDBRD_TYPE(index, sub)), e.g. "WMSDCS", or NULL.
const char *shim_sndbrd_name(unsigned type) {
  const unsigned idx = type >> 8;
  if (idx == 0 || idx >= sizeof(shim_boards) / sizeof(shim_boards[0]) || !shim_boards[idx])
    return NULL;
  return shim_boards[idx]->typestr;
}

// The name of a REGION_* value ("cpu1", "sound1", "user2"...), or NULL.
const char *shim_region_name(unsigned r) {
  static char buf[16];
  if (r >= REGION_CPU1 && r < REGION_GFX1) { sprintf(buf, "cpu%u", r - REGION_CPU1 + 1); return buf; }
  if (r >= REGION_GFX1 && r < REGION_PROMS) { sprintf(buf, "gfx%u", r - REGION_GFX1 + 1); return buf; }
  if (r == REGION_PROMS) return "proms";
  if (r >= REGION_SOUND1 && r < REGION_USER1) { sprintf(buf, "sound%u", r - REGION_SOUND1 + 1); return buf; }
  if (r >= REGION_USER1 && r < REGION_DISKS) { sprintf(buf, "user%u", r - REGION_USER1 + 1); return buf; }
  if (r == REGION_DISKS) return "disks";
  return NULL;
}

// ---------------------------------------------------------------------------------------
// Pinball 2000 (SNDBRD_DCSP2K): the words between the game and the DCS2 board.
//
// The P2K game is a PC (src/p2k) whose bus calls the board's 16-bit host interface
// directly (p2k.c p2k_dcs_write -> wmssnd.c dcs_p2k_data_w), not through sndbrd_data_w:
// libpinmame's sound command callback never sees a word. The board's DSP, though, is a
// PinMAME CPU whose data map has the host latches (wmssnd.c dcs3_readmem/writemem): data
// 0400 reads the host's word and a write there acknowledges it (the next queued word is
// then latched), a write to 0401 is the DSP's reply. The hooks below sit in front of
// those handlers, as the SAM one does: every word the DSP takes from the host (the word
// it last read at 0400 when it acknowledges) and every reply is logged with the emulated
// time, then the board's own handler runs. Nothing in PinMAME is changed.

#define SHIM_P2K_LOG 65536
static mem_read16_handler shim_p2k_in_r_orig;
static mem_write16_handler shim_p2k_ack_w_orig, shim_p2k_out_w_orig;
static int shim_p2k_cpu = -1;
static data16_t shim_p2k_last_in;
static volatile unsigned shim_p2k_n, shim_p2k_lost;
static struct { double at; unsigned short word; unsigned char reply; } shim_p2k_log[SHIM_P2K_LOG];

static void shim_p2k_push(unsigned short w, int reply) {
  if (shim_p2k_n >= SHIM_P2K_LOG) { shim_p2k_lost++; return; }
  shim_p2k_log[shim_p2k_n].at = timer_get_time();
  shim_p2k_log[shim_p2k_n].word = w;
  shim_p2k_log[shim_p2k_n].reply = (unsigned char)reply;
  shim_p2k_n++;
}

static READ16_HANDLER(shim_p2k_in_r) {
  data16_t v = shim_p2k_in_r_orig(offset, mem_mask);
  shim_p2k_last_in = v;
  return v;
}
static WRITE16_HANDLER(shim_p2k_ack_w) {
  shim_p2k_push(shim_p2k_last_in, 0);
  shim_p2k_ack_w_orig(offset, data, mem_mask);
}
static WRITE16_HANDLER(shim_p2k_out_w) {
  shim_p2k_push((unsigned short)data, 1);
  shim_p2k_out_w_orig(offset, data, mem_mask);
}

// Puts the hooks in front of the DCS2 board's latch handlers. Call it from the emulation
// thread between two frames. Returns 1 when hooked (or already hooked), 0 when the machine
// has no SNDBRD_DCSP2K board or its DSP's map is not the one wmssnd.c declares.
int shim_p2k_hook(void) {
  int ii;
  const offs_t in = ADSP2100_DATA_OFFSET + (0x0400 << 1), out = ADSP2100_DATA_OFFSET + (0x0401 << 1);
  if (shim_p2k_cpu >= 0)
    return 1;
  if (!sndbrd_exists(0) || sndbrd_type(0) != SNDBRD_DCSP2K)
    return 0;
  for (ii = 0; ii < MAX_CPU; ii++) {
    const struct Memory_ReadAddress16 *r;
    const struct Memory_WriteAddress16 *w;
    mem_read16_handler in_r = NULL;
    mem_write16_handler ack_w = NULL, out_w = NULL;
    if (!Machine->drv->cpu[ii].cpu_type || !(Machine->drv->cpu[ii].cpu_flags & CPU_AUDIO_CPU))
      continue;
    r = (const struct Memory_ReadAddress16 *)Machine->drv->cpu[ii].memory_read;
    w = (const struct Memory_WriteAddress16 *)Machine->drv->cpu[ii].memory_write;
    for (; r && !IS_MEMPORT_END(r); r++)
      if (!IS_MEMPORT_MARKER(r) && r->start == in && r->end == in + 1)
        in_r = r->handler;
    for (; w && !IS_MEMPORT_END(w); w++)
      if (!IS_MEMPORT_MARKER(w) && w->start == in && w->end == in + 1)
        ack_w = w->handler;
      else if (!IS_MEMPORT_MARKER(w) && w->start == out && w->end == out + 1)
        out_w = w->handler;
    if (!in_r || !ack_w || !out_w)
      continue;
    shim_p2k_in_r_orig = in_r;
    shim_p2k_ack_w_orig = ack_w;
    shim_p2k_out_w_orig = out_w;
    shim_p2k_n = shim_p2k_lost = 0;
    install_mem_read16_handler(ii, in, in + 1, shim_p2k_in_r);
    install_mem_write16_handler(ii, in, in + 1, shim_p2k_ack_w);
    install_mem_write16_handler(ii, out, out + 1, shim_p2k_out_w);
    shim_p2k_cpu = ii;
    return 1;
  }
  return 0;
}

// Moves the logged words out (oldest first, at most `max`), and empties the log. Returns
// how many were stored; `lost` gets how many did not fit in the log since the last call.
int shim_p2k_take(double *at, unsigned short *word, unsigned char *reply, int max, unsigned *lost) {
  unsigned i, n = shim_p2k_n;
  if (n > (unsigned)max)
    n = (unsigned)max;
  for (i = 0; i < n; i++) {
    at[i] = shim_p2k_log[i].at;
    word[i] = shim_p2k_log[i].word;
    reply[i] = shim_p2k_log[i].reply;
  }
  if (n < shim_p2k_n)
    memmove(shim_p2k_log, shim_p2k_log + n, (shim_p2k_n - n) * sizeof shim_p2k_log[0]);
  shim_p2k_n -= n;
  *lost = shim_p2k_lost;
  shim_p2k_lost = 0;
  return (int)n;
}

// Sends one 16-bit word to the board's host port, as the game's PC does (p2k.c
// p2k_dcs_write, a word write at BAR4 offset 0). sndbrd_manCmd would truncate it to 8 bits.
void shim_p2k_word(unsigned short w) {
  dcs_p2k_data_w(w);
}

// Takes the DCS2 board's pending reply, as the game's PC does (a word read at BAR4 offset
// 0 once the status says one is there). The DSP waits for the host to take each reply
// before it goes on (its boot after a reset answers EE07, then waits). Returns 1 when a
// reply was taken.
int shim_p2k_take_reply(void) {
  if (shim_p2k_cpu < 0 || !(dcs_p2k_status_r() & 0x80))
    return 0;
  dcs_p2k_data_r();
  return 1;
}
