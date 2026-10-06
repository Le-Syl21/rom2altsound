//! Just enough of the ZIP format to read one member of a ROM zip (stored or deflated):
//! the central directory, then the member's data, inflated and checked against its CRC32.
//! Used for the Stern SAM flash image, which rom2altsound reads itself (no emulation).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One member, as the central directory lists it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub crc32: u32,
    pub compressed: u64,
    pub size: u64,
    local_header: u64,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// The members of a zip file.
pub fn list(path: &Path) -> Result<Vec<Entry>, String> {
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut f = File::open(path).map_err(err)?;
    let len = f.seek(SeekFrom::End(0)).map_err(err)?;
    // The end of central directory record is in the last 22 + 65535 bytes.
    let tail_len = len.min(22 + 65535);
    let mut tail = vec![0u8; tail_len as usize];
    f.seek(SeekFrom::Start(len - tail_len)).map_err(err)?;
    f.read_exact(&mut tail).map_err(err)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..i + 4] == *b"PK\x05\x06")
        .ok_or_else(|| format!("{}: not a zip file", path.display()))?;
    let count = u16_at(&tail, eocd + 10) as usize;
    let cd_size = u32_at(&tail, eocd + 12) as u64;
    let cd_off = u32_at(&tail, eocd + 16) as u64;
    if cd_off == 0xFFFF_FFFF || cd_size == 0xFFFF_FFFF {
        return Err(format!("{}: zip64 is not supported", path.display()));
    }
    let mut cd = vec![0u8; cd_size as usize];
    f.seek(SeekFrom::Start(cd_off)).map_err(err)?;
    f.read_exact(&mut cd).map_err(err)?;
    let mut out = Vec::with_capacity(count);
    let mut p = 0;
    while p + 46 <= cd.len() && cd[p..p + 4] == *b"PK\x01\x02" {
        let name_len = u16_at(&cd, p + 28) as usize;
        let extra_len = u16_at(&cd, p + 30) as usize;
        let comment_len = u16_at(&cd, p + 32) as usize;
        let name =
            String::from_utf8_lossy(cd.get(p + 46..p + 46 + name_len).unwrap_or(&[])).into_owned();
        out.push(Entry {
            name,
            method: u16_at(&cd, p + 10),
            crc32: u32_at(&cd, p + 16),
            compressed: u32_at(&cd, p + 20) as u64,
            size: u32_at(&cd, p + 24) as u64,
            local_header: u32_at(&cd, p + 42) as u64,
        });
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// Reads one member, inflated, and checks its CRC32.
pub fn read(path: &Path, e: &Entry) -> Result<Vec<u8>, String> {
    let err = |m: String| format!("{} ({}): {m}", path.display(), e.name);
    let mut f = File::open(path).map_err(|x| err(x.to_string()))?;
    let mut lh = [0u8; 30];
    f.seek(SeekFrom::Start(e.local_header))
        .and_then(|_| f.read_exact(&mut lh))
        .map_err(|x| err(x.to_string()))?;
    if lh[..4] != *b"PK\x03\x04" {
        return Err(err("bad local header".into()));
    }
    let skip = 30 + u16_at(&lh, 26) as u64 + u16_at(&lh, 28) as u64;
    let mut data = vec![0u8; e.compressed as usize];
    f.seek(SeekFrom::Start(e.local_header + skip))
        .and_then(|_| f.read_exact(&mut data))
        .map_err(|x| err(x.to_string()))?;
    let bytes = match e.method {
        0 => data,
        8 => {
            let out = miniz_oxide::inflate::decompress_to_vec_with_limit(&data, e.size as usize)
                .map_err(|x| err(format!("inflate: {x:?}")))?;
            drop(data);
            out
        }
        m => return Err(err(format!("compression method {m} is not supported"))),
    };
    if bytes.len() as u64 != e.size {
        return Err(err(format!("{} bytes, {} expected", bytes.len(), e.size)));
    }
    let crc = crc32(&bytes);
    if crc != e.crc32 {
        return Err(err(format!("CRC32 {crc:08x}, {:08x} expected", e.crc32)));
    }
    Ok(bytes)
}

/// CRC-32 (IEEE, as zip uses it).
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    !data.iter().fold(!0u32, |c, &b| {
        t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn reads_a_stored_member() {
        // A one-member zip, method 0, built by hand.
        let data = b"hello sam";
        let crc = crc32(data);
        let name = b"a.bin";
        let mut z = Vec::new();
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z.extend_from_slice(name);
        z.extend_from_slice(data);
        let cd = z.len() as u32;
        z.extend_from_slice(b"PK\x01\x02");
        z.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0; 12]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name);
        let cd_size = z.len() as u32 - cd;
        z.extend_from_slice(b"PK\x05\x06");
        z.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&cd_size.to_le_bytes());
        z.extend_from_slice(&cd.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        let dir = std::env::temp_dir().join(format!("rom2altsound-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.zip");
        std::fs::write(&p, &z).unwrap();
        let l = list(&p).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "a.bin");
        assert_eq!(read(&p, &l[0]).unwrap(), data);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
