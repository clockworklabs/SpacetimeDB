use super::*;
use std::io::Write;

fn archive() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_ustar();
    header.set_path("usr/bin/main").unwrap();
    header.set_size(4);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append(&header, &b"data"[..]).unwrap();
    builder.into_inner().unwrap()
}
fn descriptor(bytes: &[u8], media: &str) -> Descriptor {
    Descriptor {
        digest: crate::sha256(bytes),
        size: bytes.len() as u64,
        media_type: media.into(),
        platform: None,
        urls: vec![],
        data: None,
        artifact_type: None,
    }
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}
#[test]
fn verifies_plain_gzip_and_zstd_against_actual_expanded_bytes() {
    let tar = archive();
    let diff = crate::sha256(&tar);
    for (bytes, media) in [
        (tar.clone(), "application/vnd.oci.image.layer.v1.tar"),
        (gzip(&tar), "application/vnd.oci.image.layer.v1.tar+gzip"),
        (
            zstd::stream::encode_all(&tar[..], 1).unwrap(),
            "application/vnd.oci.image.layer.v1.tar+zstd",
        ),
    ] {
        let result = verify_layer(&bytes[..], &descriptor(&bytes, media), diff, LayerLimits::default()).unwrap();
        assert_eq!(result.uncompressed_tar_bytes, tar.len() as u64);
        assert_eq!(result.entries, 1);
        assert_eq!(result.regular_file_bytes, 4);
        assert!(
            VerifiedImageSize::from_layers(bytes.len() as u64, &[result])
                .unwrap()
                .cache_reservation_bytes
                > result.uncompressed_tar_bytes
        );
    }
}
#[test]
fn decompression_bomb_is_stopped_before_declared_diff_id_can_be_trusted() {
    let expanded = vec![0u8; 1024 * 1024];
    let compressed = gzip(&expanded);
    let result = verify_layer(
        &compressed[..],
        &descriptor(&compressed, "application/vnd.oci.image.layer.v1.tar+gzip"),
        crate::sha256(&expanded),
        LayerLimits {
            max_uncompressed_bytes: 4096,
            ..LayerLimits::default()
        },
    );
    assert!(result.unwrap_err().to_string().contains("byte bound"));
}
#[test]
fn catches_digest_mismatch_truncation_and_entry_limit() {
    let bytes = archive();
    let descriptor = descriptor(&bytes, "application/vnd.oci.image.layer.v1.tar");
    assert!(verify_layer(&bytes[..], &descriptor, crate::sha256(b"wrong"), LayerLimits::default()).is_err());
    assert!(verify_layer(
        &bytes[..100],
        &descriptor,
        crate::sha256(&bytes),
        LayerLimits::default()
    )
    .is_err());
    assert!(verify_layer(
        &bytes[..],
        &descriptor,
        crate::sha256(&bytes),
        LayerLimits {
            max_entries: 0,
            ..LayerLimits::default()
        }
    )
    .is_err());
}
#[test]
fn extension_size_is_bounded_before_allocating_and_sparse_metadata_is_rejected() {
    let mut header = tar::Header::new_gnu();
    header.set_path("pax").unwrap();
    header.set_entry_type(tar::EntryType::XHeader);
    header.set_size(1 << 30);
    header.set_cksum();
    let bytes = header.as_bytes().to_vec();
    assert!(verify_layer(
        &bytes[..],
        &descriptor(&bytes, "application/vnd.oci.image.layer.v1.tar"),
        crate::sha256(&bytes),
        LayerLimits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("metadata entry"));
    assert!(parse_pax(b"25 GNU.sparse.size=123456\n").is_err());
    assert!(validate_path(b"safe/../../host", 4096, false).is_err());
    assert!(validate_path(b"/absolute", 4096, false).is_err());
}
#[test]
fn counts_concatenated_gzip_members_and_checks_pax_sizes() {
    let tar = archive();
    let mut bytes = gzip(&tar);
    bytes.extend(gzip(&tar));
    let mut both = tar.clone();
    both.extend(&tar);
    let result = verify_layer(
        &bytes[..],
        &descriptor(&bytes, "application/vnd.oci.image.layer.v1.tar+gzip"),
        crate::sha256(&both),
        LayerLimits::default(),
    )
    .unwrap();
    assert_eq!(result.entries, 2);
    assert_eq!(parse_pax(b"10 size=4\n").unwrap()["size"], "4");
    assert!(parse_pax(b"99 size=4\n").is_err());
}
