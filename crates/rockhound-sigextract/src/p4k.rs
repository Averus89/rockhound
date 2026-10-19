//! Minimal reader for Star Citizen's `Data.p4k`.
//!
//! The archive is a ZIP64 file with two CIG twists: local file headers start
//! with `PK\x03\x14` instead of `PK\x03\x04`, and most entries use
//! compression method 100 (Zstandard). Only reading one entry by name is
//! supported, which is all the extractor needs; a general ZIP crate rejects
//! both twists.

use std::io::{BufReader, Read, Seek, SeekFrom};

#[derive(Debug, thiserror::Error)]
pub enum P4kError {
    #[error("I/O error reading the archive: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a ZIP/P4K archive (no end of central directory record)")]
    NotAnArchive,
    #[error("corrupt archive: {0}")]
    Corrupt(&'static str),
    #[error("{0} is not in the archive")]
    Missing(String),
    #[error("{name} uses unsupported compression method {method}")]
    Method { name: String, method: u16 },
    #[error("{0} is encrypted or not Zstandard data; this tool can't read it")]
    Encrypted(String),
    #[error("cannot decompress {name}: {reason}")]
    Zstd { name: String, reason: String },
}

const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIGS: [u32; 2] = [0x0403_4b50, 0x1403_4b50];
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
const METHOD_STORED: u16 = 0;
const METHOD_ZSTD: u16 = 100;
/// Game2.dcb is about 330 MB; never reserve more than this up front.
const MAX_PREALLOC: usize = 1 << 30;

/// One file in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    method: u16,
    compressed_size: u64,
    size: u64,
    header_offset: u64,
}

pub struct P4k<R> {
    reader: R,
    central_offset: u64,
    entries: u64,
}

impl<R: Read + Seek> P4k<R> {
    /// Locate the central directory.
    pub fn open(mut reader: R) -> Result<Self, P4kError> {
        let len = reader.seek(SeekFrom::End(0))?;
        // The end record is 22 bytes plus a comment of at most 64 KiB.
        let tail_len = len.min(22 + 0xffff);
        let tail = read_at(&mut reader, len - tail_len, tail_len as usize)?;
        let eocd = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|&i| u32_at(&tail, i) == EOCD_SIG)
            .ok_or(P4kError::NotAnArchive)?;
        let mut entries = u64::from(u16_at(&tail, eocd + 10));
        let mut central_offset = u64::from(u32_at(&tail, eocd + 16));

        // A ZIP64 locator sits right before the end record when the 32-bit
        // fields overflowed, which they always do for Data.p4k.
        if eocd >= 20 && u32_at(&tail, eocd - 20) == ZIP64_LOCATOR_SIG {
            let zip64_offset = u64_at(&tail, eocd - 20 + 8);
            let rec = read_at(&mut reader, zip64_offset, 56)?;
            if u32_at(&rec, 0) != ZIP64_EOCD_SIG {
                return Err(P4kError::Corrupt("bad ZIP64 end record"));
            }
            entries = u64_at(&rec, 32);
            central_offset = u64_at(&rec, 48);
        }
        Ok(Self {
            reader,
            central_offset,
            entries,
        })
    }

    /// First entry whose name satisfies `wanted`. Names use `/` or `\`
    /// depending on who packed the archive, so callers should accept both.
    pub fn find(&mut self, wanted: impl Fn(&str) -> bool) -> Result<Option<Entry>, P4kError> {
        self.reader.seek(SeekFrom::Start(self.central_offset))?;
        // Data.p4k has over a million entries; read the directory as a
        // stream instead of loading it whole.
        let mut dir = BufReader::with_capacity(1 << 20, &mut self.reader);
        let mut fixed = [0u8; 46];
        for _ in 0..self.entries {
            dir.read_exact(&mut fixed)?;
            if u32_at(&fixed, 0) != CENTRAL_SIG {
                return Err(P4kError::Corrupt("bad central directory entry"));
            }
            let method = u16_at(&fixed, 10);
            let name_len = usize::from(u16_at(&fixed, 28));
            let extra_len = usize::from(u16_at(&fixed, 30));
            let comment_len = usize::from(u16_at(&fixed, 32));
            let mut var = vec![0u8; name_len + extra_len + comment_len];
            dir.read_exact(&mut var)?;
            let name = String::from_utf8_lossy(&var[..name_len]);
            if !wanted(&name) {
                continue;
            }
            let mut entry = Entry {
                name: name.into_owned(),
                method,
                compressed_size: u64::from(u32_at(&fixed, 20)),
                size: u64::from(u32_at(&fixed, 24)),
                header_offset: u64::from(u32_at(&fixed, 42)),
            };
            apply_zip64_extra(&mut entry, &var[name_len..name_len + extra_len]);
            return Ok(Some(entry));
        }
        Ok(None)
    }

    /// Decompressed contents of `entry`.
    pub fn read(&mut self, entry: &Entry) -> Result<Vec<u8>, P4kError> {
        let local = read_at(&mut self.reader, entry.header_offset, 30)?;
        if !LOCAL_SIGS.contains(&u32_at(&local, 0)) {
            return Err(P4kError::Corrupt("bad local file header"));
        }
        let skip = u64::from(u16_at(&local, 26)) + u64::from(u16_at(&local, 28));
        self.reader
            .seek(SeekFrom::Start(entry.header_offset + 30 + skip))?;
        let mut raw = Vec::new();
        (&mut self.reader)
            .take(entry.compressed_size)
            .read_to_end(&mut raw)?;
        if raw.len() as u64 != entry.compressed_size {
            return Err(P4kError::Corrupt("entry runs past the end of the archive"));
        }
        match entry.method {
            METHOD_STORED => Ok(raw),
            METHOD_ZSTD => decompress(&entry.name, &raw, entry.size),
            method => Err(P4kError::Method {
                name: entry.name.clone(),
                method,
            }),
        }
    }
}

/// Read the named entry out of an archive file in one go.
pub fn read_entry<R: Read + Seek>(
    reader: R,
    wanted: impl Fn(&str) -> bool,
    label: &str,
) -> Result<(String, Vec<u8>), P4kError> {
    let mut archive = P4k::open(reader)?;
    let entry = archive
        .find(wanted)?
        .ok_or_else(|| P4kError::Missing(label.to_owned()))?;
    let data = archive.read(&entry)?;
    Ok((entry.name, data))
}

fn decompress(name: &str, raw: &[u8], size: u64) -> Result<Vec<u8>, P4kError> {
    // Encrypted entries are not Zstandard on the outside; say so instead of
    // reporting a confusing decoder error.
    if !raw.starts_with(&ZSTD_MAGIC) {
        return Err(P4kError::Encrypted(name.to_owned()));
    }
    let zstd_err = |e: &dyn std::fmt::Display| P4kError::Zstd {
        name: name.to_owned(),
        reason: e.to_string(),
    };
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(raw).map_err(|e| zstd_err(&e))?;
    // The directory's size is only a hint until the data proves it.
    let mut out = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(MAX_PREALLOC));
    decoder.read_to_end(&mut out).map_err(|e| zstd_err(&e))?;
    if out.len() as u64 != size {
        return Err(zstd_err(&format!(
            "decompressed to {} bytes, archive says {size}",
            out.len()
        )));
    }
    Ok(out)
}

/// Replace 32-bit sizes and offsets marked as overflowed (all ones) with the
/// 64-bit values from the ZIP64 extra field, which lists them in this order.
fn apply_zip64_extra(entry: &mut Entry, mut extra: &[u8]) {
    while extra.len() >= 4 {
        let id = u16_at(extra, 0);
        let len = usize::from(u16_at(extra, 2)).min(extra.len() - 4);
        if id == 0x0001 {
            let mut fields = extra[4..4 + len].chunks_exact(8).map(|c| u64_at(c, 0));
            for value in [
                &mut entry.size,
                &mut entry.compressed_size,
                &mut entry.header_offset,
            ] {
                if *value == u64::from(u32::MAX) {
                    match fields.next() {
                        Some(v) => *value = v,
                        None => return,
                    }
                }
            }
            return;
        }
        extra = &extra[4 + len..];
    }
}

fn read_at<R: Read + Seek>(reader: &mut R, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;

    pub struct TestFile<'a> {
        pub name: &'a str,
        pub data: &'a [u8],
        pub method: u16,
    }

    /// Build an archive the way Data.p4k is laid out: CIG local header
    /// signature, every size and offset in the ZIP64 extra field, and a
    /// ZIP64 end record.
    pub fn build_p4k(files: &[TestFile]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for f in files {
            let payload = match f.method {
                METHOD_ZSTD => ruzstd::encoding::compress_to_vec(
                    f.data,
                    ruzstd::encoding::CompressionLevel::Fastest,
                ),
                _ => f.data.to_vec(),
            };
            let offset = out.len() as u64;
            let mut zip64 = vec![0x01, 0x00, 24, 0];
            for v in [f.data.len() as u64, payload.len() as u64, offset] {
                zip64.extend_from_slice(&v.to_le_bytes());
            }
            // Local header with a junk extra field, to prove it is skipped.
            out.extend_from_slice(&0x1403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[0u8; 4]);
            out.extend_from_slice(&f.method.to_le_bytes());
            out.extend_from_slice(&[0u8; 16]);
            out.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.extend_from_slice(f.name.as_bytes());
            out.extend_from_slice(&[9, 9, 9]);
            out.extend_from_slice(&payload);

            // A CIG-style extra block before the ZIP64 one.
            let mut extra = vec![0x00, 0x50, 2, 0, 7, 7];
            extra.extend_from_slice(&zip64);
            central.extend_from_slice(&CENTRAL_SIG.to_le_bytes());
            central.extend_from_slice(&[0u8; 6]);
            central.extend_from_slice(&f.method.to_le_bytes());
            central.extend_from_slice(&[0u8; 8]);
            central.extend_from_slice(&[0xff; 8]);
            central.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            central.extend_from_slice(&(extra.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0u8; 10]);
            central.extend_from_slice(&[0xff; 4]);
            central.extend_from_slice(f.name.as_bytes());
            central.extend_from_slice(&extra);
        }
        let central_offset = out.len() as u64;
        out.extend_from_slice(&central);
        let zip64_offset = out.len() as u64;
        out.extend_from_slice(&ZIP64_EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&[0u8; 28]);
        out.extend_from_slice(&(files.len() as u64).to_le_bytes());
        out.extend_from_slice(&(central.len() as u64).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&ZIP64_LOCATOR_SIG.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&zip64_offset.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&[0u8; 6]);
        out.extend_from_slice(&[0xff; 14]);
        out.extend_from_slice(&[0u8; 2]);
        out
    }

    fn files() -> Vec<TestFile<'static>> {
        vec![
            TestFile {
                name: "Data\\Localization\\global.ini",
                data: b"hello",
                method: METHOD_STORED,
            },
            TestFile {
                name: "Data\\Game2.dcb",
                data: b"datacore datacore datacore",
                method: METHOD_ZSTD,
            },
        ]
    }

    #[test]
    fn reads_stored_and_zstd_entries() {
        let archive = build_p4k(&files());
        let (name, data) = read_entry(
            Cursor::new(&archive),
            |n| n.ends_with("Game2.dcb"),
            "Game2.dcb",
        )
        .unwrap();
        assert_eq!(name, "Data\\Game2.dcb");
        assert_eq!(data, b"datacore datacore datacore");
        let (_, data) = read_entry(Cursor::new(&archive), |n| n.ends_with(".ini"), "ini").unwrap();
        assert_eq!(data, b"hello");
    }

    #[test]
    fn reports_missing_entry() {
        let archive = build_p4k(&files());
        let err = read_entry(Cursor::new(&archive), |_| false, "Game2.dcb").unwrap_err();
        assert!(matches!(err, P4kError::Missing(n) if n == "Game2.dcb"));
    }

    #[test]
    fn rejects_non_archives() {
        let err = P4k::open(Cursor::new(vec![0u8; 100])).err().unwrap();
        assert!(matches!(err, P4kError::NotAnArchive));
    }

    #[test]
    fn rejects_encrypted_and_unknown_methods() {
        let archive = build_p4k(&[
            TestFile {
                name: "secret",
                data: b"not zstd",
                method: METHOD_STORED,
            },
            TestFile {
                name: "deflated",
                data: b"x",
                method: 8,
            },
        ]);
        let mut p4k = P4k::open(Cursor::new(&archive)).unwrap();
        let mut entry = p4k.find(|n| n == "secret").unwrap().unwrap();
        entry.method = METHOD_ZSTD;
        assert!(matches!(p4k.read(&entry), Err(P4kError::Encrypted(_))));
        let entry = p4k.find(|n| n == "deflated").unwrap().unwrap();
        assert!(matches!(
            p4k.read(&entry),
            Err(P4kError::Method { method: 8, .. })
        ));
    }

    #[test]
    fn rejects_a_size_mismatch() {
        let archive = build_p4k(&files());
        let mut p4k = P4k::open(Cursor::new(&archive)).unwrap();
        let mut entry = p4k.find(|n| n.ends_with("Game2.dcb")).unwrap().unwrap();
        entry.size += 1;
        assert!(matches!(p4k.read(&entry), Err(P4kError::Zstd { .. })));
    }

    #[test]
    fn rejects_corrupt_headers() {
        let mut archive = build_p4k(&files());
        let mut p4k = P4k::open(Cursor::new(archive.clone())).unwrap();
        let mut entry = p4k.find(|n| n.ends_with(".ini")).unwrap().unwrap();
        entry.header_offset += 1;
        assert!(matches!(p4k.read(&entry), Err(P4kError::Corrupt(_))));
        entry.header_offset -= 1;
        entry.compressed_size = 1 << 40;
        assert!(matches!(p4k.read(&entry), Err(P4kError::Corrupt(_))));

        let central = archive
            .windows(4)
            .position(|w| w == CENTRAL_SIG.to_le_bytes())
            .unwrap();
        archive[central] = 0;
        let mut p4k = P4k::open(Cursor::new(archive)).unwrap();
        assert!(matches!(p4k.find(|_| true), Err(P4kError::Corrupt(_))));
    }
}
