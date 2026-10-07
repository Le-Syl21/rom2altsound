//! The state of the sound CPUs, frame by frame: their registers and RAM, read between two
//! emulated frames. On boards whose music is played by a program on the sound CPU (every
//! board but DCS), the music has looped when this state comes back, even where the audio
//! does not repeat sample-exactly.

use std::ffi::c_int;

use crate::ffi;

/// Registers read per CPU: 1..=9 covers the 6809 (PC, S, CC, A, B, U, X, Y, DP) and the
/// 6800/6803 (PC, S, A, B, X, CC, then the WAI/NMI/IRQ states).
const REGS: c_int = 9;

/// Where the state of each audio CPU is read.
pub struct Probe {
    cpus: Vec<(c_int, Vec<(u32, u32)>)>,
    /// Bytes per snapshot.
    pub len: usize,
    /// Each snapshot byte's origin, for reports: (cpu, address); registers have
    /// address `u32::MAX - reg`.
    pub origin: Vec<(i32, u32)>,
}

impl Probe {
    /// The machine's audio CPUs with an 8-bit bus and their RAM. None when there is none
    /// (only valid once the machine runs, on the emulation thread).
    pub fn new() -> Option<Probe> {
        let mut cpus = Vec::new();
        let mut origin = Vec::new();
        for i in 0.. {
            let cpu = unsafe { ffi::shim_audio_cpu(i) };
            if cpu < 0 {
                break;
            }
            let (mut st, mut en) = ([0u32; 16], [0u32; 16]);
            let n = unsafe { ffi::shim_cpu_ram_ranges(cpu, st.as_mut_ptr(), en.as_mut_ptr(), 16) };
            let mut probe = Vec::new();
            let ranges: Vec<(u32, u32)> = (0..n.clamp(0, 16) as usize)
                .map(|k| (st[k], en[k]))
                .filter(|&(s, e)| {
                    probe.resize((e.saturating_sub(s) + 1) as usize, 0);
                    e >= s
                        && unsafe { ffi::shim_cpu_read(cpu, s, e - s + 1, probe.as_mut_ptr()) } != 0
                })
                .collect();
            for r in 1..=REGS {
                origin.push((cpu, u32::MAX - r as u32));
                origin.push((cpu, u32::MAX - r as u32));
            }
            for &(s, e) in &ranges {
                origin.extend((s..=e).map(|a| (cpu, a)));
            }
            cpus.push((cpu, ranges));
        }
        (!cpus.is_empty()).then_some(Probe {
            len: origin.len(),
            origin,
            cpus,
        })
    }

    /// Appends one snapshot (`len` bytes) to `out`.
    pub fn snapshot(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.resize(start + self.len, 0);
        let mut at = start;
        for (cpu, ranges) in &self.cpus {
            for r in 1..=REGS {
                let v = unsafe { ffi::shim_cpu_reg(*cpu, r) } as u16;
                out[at..at + 2].copy_from_slice(&v.to_le_bytes());
                at += 2;
            }
            for &(s, e) in ranges {
                let n = (e - s + 1) as usize;
                unsafe { ffi::shim_cpu_read(*cpu, s, n as u32, out[at..].as_mut_ptr()) };
                at += n;
            }
        }
    }

    /// One line per CPU: its number and RAM ranges.
    pub fn describe(&self) -> String {
        self.cpus
            .iter()
            .map(|(cpu, r)| {
                let ranges: Vec<String> =
                    r.iter().map(|(s, e)| format!("{s:04X}-{e:04X}")).collect();
                format!("cpu {cpu}: RAM {}", ranges.join(" "))
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
