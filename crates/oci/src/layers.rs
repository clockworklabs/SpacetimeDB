//! Stream-verify layer expansion without extracting anything onto the host.
//! Resource admission uses measured bytes and bounded metadata, never compressed
//! sizes alone. The runtime must also enforce a dedicated finite cache filesystem.

use crate::{Descriptor, OciDigest};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

#[derive(Clone, Copy, Debug)]
pub struct LayerLimits {
    pub max_uncompressed_bytes: u64,
    pub max_regular_file_bytes: u64,
    pub max_entries: u64,
    pub max_metadata_bytes: usize,
    pub max_path_bytes: usize,
    pub zstd_window_log_max: u32,
}
impl Default for LayerLimits {
    fn default() -> Self {
        Self {
            max_uncompressed_bytes: 128 * 1024 * 1024 * 1024,
            max_regular_file_bytes: 64 * 1024 * 1024 * 1024,
            max_entries: 1_000_000,
            max_metadata_bytes: 64 * 1024,
            max_path_bytes: 4096,
            zstd_window_log_max: 27,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct VerifiedLayerSize {
    pub uncompressed_tar_bytes: u64,
    pub entries: u64,
    pub regular_file_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VerifiedImageSize {
    pub compressed_bytes: u64,
    pub uncompressed_tar_bytes: u64,
    pub entries: u64,
    pub cache_reservation_bytes: u64,
}
impl VerifiedImageSize {
    /// Conservative cache accounting, supplemented by the backend's hard
    /// filesystem bound. Includes temporary compressed/unpacked copies and
    /// per-entry/per-layer filesystem metadata. The trusted artifact service
    /// supplies these verified measurements, never a publisher JSON field.
    pub fn from_layers(compressed_bytes: u64, layers: &[VerifiedLayerSize]) -> Result<Self> {
        let mut tar = 0u64;
        let mut entries = 0u64;
        for layer in layers {
            tar = tar
                .checked_add(layer.uncompressed_tar_bytes)
                .context("expanded image size overflow")?;
            entries = entries
                .checked_add(layer.entries)
                .context("image entry count overflow")?;
        }
        let reservation = compressed_bytes
            .checked_mul(2)
            .and_then(|n| tar.checked_mul(3).and_then(|v| n.checked_add(v)))
            .and_then(|n| entries.checked_mul(64 * 1024).and_then(|v| n.checked_add(v)))
            .and_then(|n| {
                (layers.len() as u64)
                    .checked_mul(16 * 1024 * 1024)
                    .and_then(|v| n.checked_add(v))
            })
            .context("image cache reservation overflow")?;
        Ok(Self {
            compressed_bytes,
            uncompressed_tar_bytes: tar,
            entries,
            cache_reservation_bytes: reservation,
        })
    }
}

struct HashBounded<R> {
    inner: R,
    hash: Sha256,
    count: u64,
    limit: u64,
}
impl<R: Read> Read for HashBounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.count == self.limit {
            let mut extra = [0u8; 1];
            if self.inner.read(&mut extra)? == 0 {
                return Ok(0);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "layer exceeds configured byte bound",
            ));
        }
        let maximum = buf.len().min((self.limit - self.count).min(usize::MAX as u64) as usize);
        let count = self.inner.read(&mut buf[..maximum])?;
        self.count += count as u64;
        self.hash.update(&buf[..count]);
        Ok(count)
    }
}

pub fn verify_layer(
    reader: impl Read,
    descriptor: &Descriptor,
    diff_id: OciDigest,
    limits: LayerLimits,
) -> Result<VerifiedLayerSize> {
    verify_layer_with_check(reader, descriptor, diff_id, limits, || Ok(()))
}

/// Check cancellation/deadline between both compressed and expanded reads.
/// The caller retains its worker admission until this synchronous operation ends.
pub fn verify_layer_with_check(
    reader: impl Read,
    descriptor: &Descriptor,
    diff_id: OciDigest,
    limits: LayerLimits,
    check: impl Fn() -> io::Result<()> + Copy,
) -> Result<VerifiedLayerSize> {
    struct Checked<R, F> {
        reader: R,
        check: F,
    }
    impl<R: Read, F: Fn() -> io::Result<()>> Read for Checked<R, F> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            (self.check)()?;
            self.reader.read(buf)
        }
    }
    check()?;
    ensure!(
        descriptor.size > 0 && descriptor.size <= crate::MAX_IMAGE_BYTES,
        "invalid compressed layer size"
    );
    ensure!(
        descriptor.urls.is_empty() && descriptor.data.is_none(),
        "external layer sources are unsupported"
    );
    let mut compressed = HashBounded {
        inner: Checked { reader, check },
        hash: Sha256::new(),
        count: 0,
        limit: descriptor.size,
    };
    let decoder: Box<dyn Read + '_> = match descriptor.media_type.as_str() {
        "application/vnd.oci.image.layer.v1.tar" | "application/vnd.docker.image.rootfs.diff.tar" => {
            Box::new(&mut compressed)
        }
        "application/vnd.oci.image.layer.v1.tar+gzip" | "application/vnd.docker.image.rootfs.diff.tar.gzip" => {
            Box::new(flate2::read::MultiGzDecoder::new(&mut compressed))
        }
        "application/vnd.oci.image.layer.v1.tar+zstd" => {
            let mut decoder = zstd::stream::read::Decoder::new(&mut compressed)?;
            decoder.window_log_max(limits.zstd_window_log_max)?;
            Box::new(decoder)
        }
        _ => bail!("unsupported or foreign layer media type"),
    };
    let mut expanded = HashBounded {
        inner: Checked { reader: decoder, check },
        hash: Sha256::new(),
        count: 0,
        limit: limits.max_uncompressed_bytes,
    };
    let (entries, regular_file_bytes) = scan_tar(&mut expanded, limits)?;
    let uncompressed_tar_bytes = expanded.count;
    ensure!(
        OciDigest::sha256(expanded.hash.clone().finalize().into()) == diff_id,
        "layer uncompressed SHA-256 does not match rootfs diff ID"
    );
    drop(expanded);
    ensure!(
        compressed.count == descriptor.size
            && OciDigest::sha256(compressed.hash.finalize().into()) == descriptor.digest,
        "compressed layer SHA-256 or length mismatch"
    );
    Ok(VerifiedLayerSize {
        uncompressed_tar_bytes,
        entries,
        regular_file_bytes,
    })
}

fn scan_tar(reader: &mut impl Read, limits: LayerLimits) -> Result<(u64, u64)> {
    let mut entries = 0u64;
    let mut regular_bytes = 0u64;
    let mut pax = BTreeMap::new();
    let mut global = BTreeMap::new();
    let mut long_path = None;
    let mut long_link = None;
    loop {
        let mut block = [0u8; 512];
        let first = reader.read(&mut block[..1])?;
        if first == 0 {
            break;
        }
        reader.read_exact(&mut block[1..]).context("truncated TAR header")?;
        if block.iter().all(|&b| b == 0) {
            continue;
        }
        entries = entries.checked_add(1).context("TAR entry count overflow")?;
        ensure!(entries <= limits.max_entries, "too many layer entries");
        let header = tar::Header::from_byte_slice(&block);
        let checksum = block[..148]
            .iter()
            .chain(&block[156..])
            .map(|&b| u32::from(b))
            .sum::<u32>()
            + 8 * 32;
        ensure!(header.cksum()? == checksum, "invalid TAR header checksum");
        let kind = header.entry_type().as_byte();
        let header_size = header.entry_size()?;
        if matches!(kind, b'x' | b'g' | b'L' | b'K') {
            ensure!(
                header_size <= limits.max_metadata_bytes as u64,
                "TAR metadata entry is too large"
            );
            let mut bytes = vec![0; header_size as usize];
            reader.read_exact(&mut bytes)?;
            skip_padding(reader, header_size)?;
            match kind {
                b'x' => {
                    ensure!(pax.is_empty(), "duplicate local PAX metadata");
                    pax = parse_pax(&bytes)?;
                }
                b'g' => {
                    let update = parse_pax(&bytes)?;
                    global.extend(update);
                    ensure!(
                        global.len() <= 128
                            && global.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
                                <= limits.max_metadata_bytes,
                        "global PAX metadata exceeds bound"
                    );
                }
                b'L' => {
                    ensure!(long_path.is_none(), "duplicate GNU long path");
                    long_path = Some(trim_nul(bytes));
                }
                b'K' => {
                    ensure!(long_link.is_none(), "duplicate GNU long link");
                    long_link = Some(trim_nul(bytes));
                }
                _ => unreachable!(),
            }
            continue;
        }
        let property = |name: &str| pax.get(name).or_else(|| global.get(name));
        let size = property("size")
            .map(|v| v.parse::<u64>())
            .transpose()?
            .unwrap_or(header_size);
        ensure!(size <= limits.max_regular_file_bytes, "layer file exceeds size bound");
        let raw_path = header.path_bytes();
        let path = property("path")
            .map(String::as_bytes)
            .or(long_path.as_deref())
            .unwrap_or(&raw_path);
        validate_path(path, limits.max_path_bytes, kind == b'5')?;
        match kind {
            0 | b'0' | b'7' => {
                regular_bytes = regular_bytes.checked_add(size).context("layer file size overflow")?;
            }
            b'5' => ensure!(size == 0, "directory entry has data"),
            b'1' | b'2' => {
                ensure!(size == 0, "link entry has data");
                let raw_link = header.link_name_bytes();
                let link = property("linkpath")
                    .map(String::as_bytes)
                    .or(long_link.as_deref())
                    .or(raw_link.as_deref())
                    .context("link target is missing")?;
                ensure!(
                    !link.is_empty() && link.len() <= limits.max_path_bytes && !link.contains(&0),
                    "invalid link target"
                );
                if kind == b'1' {
                    validate_path(link, limits.max_path_bytes, false)?;
                }
                // Absolute symbolic links are normal inside a Linux image.
                // Extraction remains the confined runtime unpacker's job.
            }
            _ => bail!("unsupported sparse, special-device, or unknown TAR entry type"),
        }
        skip_exact(reader, size)?;
        skip_padding(reader, size)?;
        pax.clear();
        long_path = None;
        long_link = None;
    }
    ensure!(
        pax.is_empty() && long_path.is_none() && long_link.is_none(),
        "orphaned TAR extension metadata"
    );
    Ok((entries, regular_bytes))
}

fn trim_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    bytes
}
fn skip_exact(reader: &mut impl Read, mut bytes: u64) -> Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    while bytes > 0 {
        let count = bytes.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        bytes -= count as u64;
    }
    Ok(())
}
fn skip_padding(reader: &mut impl Read, size: u64) -> Result<()> {
    skip_exact(reader, (512 - size % 512) % 512)
}
fn validate_path(path: &[u8], max: usize, root_directory: bool) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= max && !path.contains(&0) && !path.starts_with(b"/"),
        "invalid layer entry path"
    );
    ensure!(
        !path.split(|&b| b == b'/').any(|p| p == b".."),
        "layer entry path escapes image root"
    );
    ensure!(
        root_directory || path.split(|&b| b == b'/').any(|p| !p.is_empty() && p != b"."),
        "invalid root file entry"
    );
    Ok(())
}
fn parse_pax(mut bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    while !bytes.is_empty() {
        let space = bytes.iter().position(|&b| b == b' ').context("invalid PAX record")?;
        ensure!(space > 0 && space <= 10, "invalid PAX length");
        let length = std::str::from_utf8(&bytes[..space])?.parse::<usize>()?;
        ensure!(
            length > space + 2 && length <= bytes.len() && bytes[length - 1] == b'\n',
            "invalid PAX record length"
        );
        let record = std::str::from_utf8(&bytes[space + 1..length - 1])?;
        let (key, value) = record.split_once('=').context("invalid PAX property")?;
        ensure!(
            key.len() <= 256
                && (matches!(
                    key,
                    "path" | "linkpath" | "size" | "mtime" | "atime" | "ctime" | "uid" | "gid" | "uname" | "gname"
                ) || key.starts_with("SCHILY.xattr.")),
            "unsupported sparse or unknown PAX property"
        );
        ensure!(
            result.insert(key.to_owned(), value.to_owned()).is_none() && result.len() <= 128,
            "duplicate or excessive PAX properties"
        );
        bytes = &bytes[length..];
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
