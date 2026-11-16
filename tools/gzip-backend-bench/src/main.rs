//! Isolated gzip backend benchmark; not a protocol implementation.
use std::{
    hint::black_box,
    io::{Read, Write},
    time::Instant,
};

#[cfg(any(
    all(feature = "miniz", feature = "zlib-rs"),
    all(feature = "miniz", feature = "zlib-ng"),
    all(feature = "zlib-rs", feature = "zlib-ng"),
    not(any(feature = "miniz", feature = "zlib-rs", feature = "zlib-ng")),
))]
compile_error!("select exactly one backend with --no-default-features --features BACKEND");

fn backend() -> &'static str {
    if cfg!(feature = "zlib-ng") {
        "zlib-ng"
    } else if cfg!(feature = "zlib-rs") {
        "zlib-rs"
    } else {
        "miniz"
    }
}

fn u32_bytes(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

// The same deterministic synthetic 64-byte rows as the earlier snapshot
// experiment: id/category/value/[0; 40]. Random cases fill all field bytes.
fn fixture(size: usize, snapshot: bool, random: bool) -> Vec<u8> {
    let rows = size.div_ceil(64);
    let mut data = Vec::with_capacity(rows * 64);
    let mut state = 12345u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for id in 0..rows {
        if random {
            for _ in 0..8 {
                data.extend_from_slice(&next().to_le_bytes());
            }
        } else {
            data.extend_from_slice(&(id as u64).to_le_bytes());
            data.extend_from_slice(&((id % 16) as u64).to_le_bytes());
            data.extend_from_slice(&next().to_le_bytes());
            data.extend_from_slice(&[0; 40]);
        }
    }
    // Explicit current v2 BSATN fixture envelope, not a compression tag. Keep
    // this fixed across isolated backends without linking server dependencies.
    let mut out = Vec::with_capacity(data.len() + 47);
    if snapshot {
        out.push(1); // SubscribeApplied
        u32_bytes(&mut out, 7); // request_id
        u32_bytes(&mut out, 3); // query_set_id
        u32_bytes(&mut out, 1); // table count
    } else {
        out.push(4); // TransactionUpdate
        u32_bytes(&mut out, 1); // query-set count
        u32_bytes(&mut out, 3); // query_set_id
        u32_bytes(&mut out, 1); // table count
    }
    u32_bytes(&mut out, 9);
    out.extend_from_slice(b"inventory");
    if !snapshot {
        u32_bytes(&mut out, 1); // table-update row-list count
        out.push(0); // PersistentTable
    }
    out.push(0); // FixedSize row hint
    out.extend_from_slice(&64u16.to_le_bytes());
    u32_bytes(&mut out, data.len().try_into().unwrap());
    out.extend_from_slice(&data);
    if !snapshot {
        out.push(1); // empty deletes: RowOffsets
        u32_bytes(&mut out, 0);
        u32_bytes(&mut out, 0);
    }
    out
}

fn compress(input: &[u8], level: u32) -> Vec<u8> {
    let mut writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(level));
    writer.write_all(input).unwrap();
    writer.finish().unwrap()
}

fn decompress(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    flate2::read::GzDecoder::new(input).read_to_end(&mut output).unwrap();
    output
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn measure(label: &str, input: &[u8], iterations: usize, level: u32) {
    let wire = compress(input, level);
    assert_eq!(decompress(&wire), input);
    let mut enc = Vec::new();
    let mut dec = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let encoded = black_box(compress(black_box(input), level));
        enc.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(encoded.len(), wire.len());
        drop(encoded);
        let start = Instant::now();
        let decoded = black_box(decompress(black_box(&wire)));
        dec.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(decoded, input);
        drop(decoded);
    }
    println!(
        "{},{label},{level},{},{},{iterations},{:.6},{:.6}",
        backend(),
        input.len(),
        wire.len(),
        median(enc),
        median(dec)
    );
}

fn main() {
    let mut large = false;
    let mut levels = vec![1];
    for arg in std::env::args().skip(1) {
        if arg == "--large-initial-response" {
            large = true;
        } else if let Some(value) = arg.strip_prefix("--levels=") {
            levels = value
                .split(',')
                .map(|level| level.parse::<u32>().expect("gzip level must be 0–9"))
                .collect();
            assert!(levels.iter().all(|&level| level <= 9), "gzip level must be 0–9");
        } else {
            panic!("unknown argument: {arg}");
        }
    }
    println!("backend,case,level,raw_bytes,gzip_bytes,iterations,encode_ms,decode_ms");
    for (name, size, snapshot, random) in [
        ("one-row", 64, false, false),
        ("four-rows", 256, false, false),
        ("random-four-rows", 256, false, true),
        ("snapshot-4MiB", 4 * 1024 * 1024, true, false),
    ] {
        let input = fixture(size, snapshot, random);
        for &level in &levels {
            measure(name, &input, if size < 1024 { 1000 } else { 5 }, level);
        }
    }
    if large {
        let input = fixture(500 * 1024 * 1024, true, false);
        for &level in &levels {
            measure("snapshot-500MiB", &input, 5, level);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_sizes_and_gzip_integrity() {
        for snapshot in [false, true] {
            for random in [false, true] {
                for size in [0, 64, 256, 4096] {
                    let input = fixture(size, snapshot, random);
                    assert_eq!(input.len(), size + if snapshot { 33 } else { 47 });
                    for level in [1, 2, 3, 4, 6] {
                        let mut wire = compress(&input, level);
                        assert_eq!(decompress(&wire), input);
                        *wire.last_mut().unwrap() ^= 1;
                        assert!(flate2::read::GzDecoder::new(wire.as_slice())
                            .read_to_end(&mut Vec::new())
                            .is_err());
                    }
                }
            }
        }
    }
}
