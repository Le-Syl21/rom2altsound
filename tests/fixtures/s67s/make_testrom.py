#!/usr/bin/env python3
"""Build the synthetic test ROM set `flash_l1.zip` (see README.md next to this script).

The set holds no original code: it is a tiny program written for rom2altsound's tests,
assembled here by a minimal 6800 assembler (only the opcodes used below). It is laid out
as PinMAME's `flash_l1` driver loads it (Williams System 6 CPU board + System 3-7 sound
board, `SNDBRD_S67S`):

- gamerom.716 (0x6000), green1.716 (0x7000), green2.716 (0x7800, vectors at 0xFFF8):
  the game CPU (a 6808) masks its interrupts and loops forever. rom2altsound halts the
  game CPU after the boot and drives the sound board itself, so nothing else is needed.
- sound1.716 (the sound 6808, 0x7000, mirrored up to 0xF800): on a command (PIA port B,
  CB1 rising edge), plays a square wave on the DAC (PIA port A) whose frequency and
  length depend on the command, then returns to silence. Commands without an entry in
  the tone table play nothing.

The checksums do not match PinMAME's (it only logs "WRONG CHECKSUMS" and runs the set).

Usage: make_testrom.py [OUT_DIR]   (default: this script's directory)
       make_testrom.py --check      (the committed files match this script, exit 1 if not)
Writes flash_l1.zip and expected.json (what an extraction must produce).
"""

import json
import os
import sys
import zipfile

ROM_SIZE = 0x800  # 2716 EPROMs
SOUND_CLOCK = 3579545.0 / 4.0  # the S67S 6808 (wmssnd.c, MDRV_CPU_ADD(M6808, 3579545./4.))

# Command -> (frequency in Hz, length in seconds). Distinct frequencies and lengths, so no
# two files are twins.
TONES = [
    (440.0, 0.50),
    (660.0, 1.00),
    (880.0, 1.50),
    (330.0, 2.00),
    (1000.0, 0.30),
]

# Cycles of one half period of the tone loop below, outside the delay loop, and per
# iteration of the delay loop (DEX + BNE). Counted from the loop in sound_program().
LOOP_CYCLES = 35
DELAY_CYCLES = 8


class Asm:
    """A minimal 6800 assembler: bytes, labels and relative/absolute fixups."""

    def __init__(self, origin):
        self.origin = origin
        self.code = bytearray()
        self.labels = {}
        self.fixups = []  # (offset, label, kind)

    @property
    def pc(self):
        return self.origin + len(self.code)

    def label(self, name):
        self.labels[name] = self.pc

    def emit(self, *data):
        self.code += bytes(data)

    def ext(self, opcode, addr):
        self.emit(opcode, addr >> 8, addr & 0xFF)

    def branch(self, opcode, target):
        self.emit(opcode, 0)
        self.fixups.append((len(self.code) - 1, target, "rel"))

    def word_of(self, label):
        self.emit(0, 0)
        self.fixups.append((len(self.code) - 2, label, "abs"))

    def imm_hi(self, opcode, label):
        self.emit(opcode, 0)
        self.fixups.append((len(self.code) - 1, label, "hi"))

    def resolve(self):
        for offset, label, kind in self.fixups:
            target = self.labels[label]
            if kind == "rel":
                delta = target - (self.origin + offset + 1)
                assert -128 <= delta <= 127, f"branch to {label} out of range"
                self.code[offset] = delta & 0xFF
            elif kind == "abs":
                self.code[offset : offset + 2] = target.to_bytes(2, "big")
            elif kind == "hi":
                self.code[offset] = target >> 8
        return bytes(self.code)


def tone_params(freq, secs):
    """Delay loop count and number of half periods of a tone, and its exact length."""
    half = SOUND_CLOCK / (2.0 * freq)
    delay = round((half - LOOP_CYCLES) / DELAY_CYCLES)
    assert 1 <= delay <= 0xFFFF
    cycles = LOOP_CYCLES + DELAY_CYCLES * delay
    count = round(secs * SOUND_CLOCK / cycles)
    assert 1 <= count <= 0xFFFF
    return delay, count, count * cycles / SOUND_CLOCK, SOUND_CLOCK / (2.0 * cycles)


def image(origin, program):
    """A 2716 image: the program at the start, 0xFF elsewhere, the vectors at the end."""
    rom = bytearray(b"\xff" * ROM_SIZE)
    rom[: len(program)] = program
    return rom


def set_vectors(rom, irq, swi, nmi, reset):
    rom[0x7F8:0x800] = b"".join(v.to_bytes(2, "big") for v in (irq, swi, nmi, reset))


BANNER = b"rom2altsound synthetic test ROM - not a Williams ROM - BSD-3-Clause\x00"

# Sound board RAM and PIA (wmssnd.c, s67s_readmem/s67s_writemem).
PTR, CNT, DLY, LEVEL = 0x00, 0x02, 0x04, 0x06
PIA_A, PIA_CRA, PIA_B, PIA_CRB = 0x0400, 0x0401, 0x0402, 0x0403
LEVEL_ON = 0xC0


def sound_program():
    a = Asm(0xF800)
    a.label("reset")
    a.emit(0x0F)  # SEI
    a.emit(0x8E, 0x00, 0x7F)  # LDS #$007F
    a.ext(0x7F, PIA_CRA)  # CLR CRA          (select DDRA)
    a.emit(0x86, 0xFF)  # LDAA #$FF
    a.ext(0xB7, PIA_A)  # STAA DDRA        (port A: DAC, all outputs)
    a.emit(0x86, 0x04)  # LDAA #$04
    a.ext(0xB7, PIA_CRA)  # STAA CRA         (data register)
    a.ext(0x7F, PIA_A)  # CLR DAC
    a.ext(0x7F, PIA_CRB)  # CLR CRB          (select DDRB)
    a.ext(0x7F, PIA_B)  # CLR DDRB         (port B: command, all inputs)
    a.emit(0x86, 0x07)  # LDAA #$07
    a.ext(0xB7, PIA_CRB)  # STAA CRB         (CB1 interrupt on its rising edge)
    a.emit(0x0E)  # CLI
    a.label("idle")
    a.branch(0x20, "idle")  # BRA idle

    a.label("irq")
    a.ext(0xB6, PIA_B)  # LDAA PORTB       (the command; clears the interrupt)
    a.emit(0x84, 0x1F)  # ANDA #$1F        (the five bits the game sends)
    a.emit(0x81, len(TONES))  # CMPA #count
    a.branch(0x24, "done")  # BCC done         (no tone for this command)
    a.emit(0x48, 0x48)  # ASLA, ASLA       (4 bytes per entry)
    a.emit(0x97, PTR + 1)  # STAA ptr+1
    a.imm_hi(0x86, "table")  # LDAA #>table
    a.emit(0x97, PTR)  # STAA ptr
    a.emit(0xDE, PTR)  # LDX ptr
    for i, dst in enumerate((DLY, DLY + 1, CNT, CNT + 1)):
        a.emit(0xA6, i)  # LDAA i,X
        a.emit(0x97, dst)  # STAA dst
    a.emit(0x86, LEVEL_ON)  # LDAA #on
    a.emit(0x97, LEVEL)  # STAA level
    # One half period: 3+5+2+4+4 + (4+4)*delay + 4+4+5+4 = 35 + 8*delay cycles.
    a.label("half")
    a.emit(0x96, LEVEL)  # LDAA level       3
    a.ext(0xB7, PIA_A)  # STAA DAC         5
    a.emit(0x88, LEVEL_ON)  # EORA #on         2
    a.emit(0x97, LEVEL)  # STAA level       4
    a.emit(0xDE, DLY)  # LDX dly          4
    a.label("delay")
    a.emit(0x09)  # DEX              4
    a.branch(0x26, "delay")  # BNE delay        4
    a.emit(0xDE, CNT)  # LDX cnt          4
    a.emit(0x09)  # DEX              4
    a.emit(0xDF, CNT)  # STX cnt          5
    a.branch(0x26, "half")  # BNE half         4
    a.ext(0x7F, PIA_A)  # CLR DAC
    a.label("done")
    a.emit(0x3B)  # RTI

    # The tone table, on a page of its own (its high byte is loaded alone above).
    while a.pc & 0xFF:
        a.emit(0xFF)
    a.label("table")
    for freq, secs in TONES:
        delay, count, _, _ = tone_params(freq, secs)
        a.emit(delay >> 8, delay & 0xFF, count >> 8, count & 0xFF)
    a.emit(*BANNER)
    code = a.resolve()
    rom = image(0xF800, code)
    l = a.labels
    set_vectors(rom, irq=l["irq"], swi=l["reset"], nmi=l["reset"], reset=l["reset"])
    return rom


def game_program():
    """green2.716 (0x7800, mirrored at 0xF800): mask the interrupts, loop forever."""
    a = Asm(0xF800)
    a.label("reset")
    a.emit(0x0F)  # SEI
    a.label("loop")
    a.branch(0x20, "loop")  # BRA loop
    a.emit(*BANNER)
    rom = image(0xF800, a.resolve())
    r = a.labels["reset"]
    set_vectors(rom, irq=r, swi=r, nmi=r, reset=r)
    return rom


def filler():
    return image(0, BANNER)


def build():
    """The ROM files of the set, and expected.json's content."""
    files = {
        "gamerom.716": bytes(filler()),
        "green1.716": bytes(filler()),
        "green2.716": bytes(game_program()),
        "sound1.716": bytes(sound_program()),
    }
    sounds = []
    for cmd, (freq, secs) in enumerate(TONES):
        _, _, length, actual = tone_params(freq, secs)
        sounds.append({"id": cmd, "frequency_hz": round(actual, 2), "seconds": round(length, 4)})
    expected = {
        "rom": "flash_l1",
        "comment": "Written by make_testrom.py: the sounds an extraction of flash_l1.zip must hold",
        "sounds": sounds,
    }
    return files, json.dumps(expected, indent=2) + "\n"


def check(here):
    """Compare the committed zip's members and expected.json with what build() makes."""
    files, expected = build()
    ok = True
    with zipfile.ZipFile(os.path.join(here, "flash_l1.zip")) as z:
        if sorted(z.namelist()) != sorted(files):
            print(f"flash_l1.zip holds {sorted(z.namelist())}, expected {sorted(files)}")
            ok = False
        for name, data in files.items():
            if name in z.namelist() and z.read(name) != data:
                print(f"flash_l1.zip: {name} differs from make_testrom.py's")
                ok = False
    with open(os.path.join(here, "expected.json"), newline="") as f:
        if f.read() != expected:
            print("expected.json differs from make_testrom.py's")
            ok = False
    print("fixture up to date" if ok else "run make_testrom.py and commit its output")
    return ok


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check(here) else 1)
    out = sys.argv[1] if len(sys.argv) > 1 else here
    files, expected = build()
    # Fixed timestamps: the zip is the same at every run.
    with zipfile.ZipFile(os.path.join(out, "flash_l1.zip"), "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, data)
    with open(os.path.join(out, "expected.json"), "w", newline="\n") as f:
        f.write(expected)


if __name__ == "__main__":
    main()
