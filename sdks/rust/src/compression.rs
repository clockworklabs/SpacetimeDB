use crate::websocket::WsError;
use spacetimedb_client_api_messages::websocket as ws;
use std::borrow::Cow;
use std::io::{self, Read as _};
use std::sync::Arc;

fn brotli_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut &bytes[..], &mut decompressed)?;
    Ok(decompressed)
}

fn gzip_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut decompressed = Vec::new();
    let _ = flate2::read::GzDecoder::new(bytes).read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

#[cfg(not(target_arch = "wasm32"))]
fn zstd_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    zstd::stream::decode_all(bytes)
}

// Browser targets must not depend on native libzstd. Also exercised on native
// test runs against frames produced by the same encoder as the server.
#[cfg(any(target_arch = "wasm32", test))]
fn zstd_decompress_portable(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes).map_err(io::Error::other)?;
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    // ruzstd exposes the stored/calculated checksums but does not compare them.
    if let Some(expected) = decoder.decoder.get_checksum_from_data()
        && decoder.decoder.get_calculated_checksum() != Some(expected)
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Zstd checksum mismatch"));
    }
    Ok(output)
}

#[cfg(target_arch = "wasm32")]
fn zstd_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    zstd_decompress_portable(bytes)
}

/// Decompresses a `ServerMessage` encoded in BSATN into the raw BSATN
/// for further deserialization.
pub(crate) fn decompress_server_message(raw: &[u8]) -> Result<Cow<'_, [u8]>, WsError> {
    let err_decompress = |scheme| {
        move |source| WsError::Decompress {
            scheme,
            source: Arc::new(source),
        }
    };
    match raw {
        [] => Err(WsError::EmptyMessage),
        [ws::common::SERVER_MSG_COMPRESSION_TAG_NONE, bytes @ ..] => Ok(Cow::Borrowed(bytes)),
        [ws::common::SERVER_MSG_COMPRESSION_TAG_BROTLI, bytes @ ..] => brotli_decompress(bytes)
            .map(Cow::Owned)
            .map_err(err_decompress("brotli")),
        [ws::common::SERVER_MSG_COMPRESSION_TAG_GZIP, bytes @ ..] => {
            gzip_decompress(bytes).map(Cow::Owned).map_err(err_decompress("gzip"))
        }
        [ws::common::SERVER_MSG_COMPRESSION_TAG_ZSTD, bytes @ ..] => {
            zstd_decompress(bytes).map(Cow::Owned).map_err(err_decompress("zstd"))
        }
        [c, ..] => Err(WsError::UnknownCompressionScheme { scheme: *c }),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::io::Write;

    fn encode(bytes: &[u8], workers: u32) -> Vec<u8> {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), -2).unwrap();
        encoder.include_checksum(true).unwrap();
        if workers > 0 {
            encoder.multithread(workers).unwrap();
        }
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn zstd_frames_decode_on_native_and_portable_paths() {
        let message = ws::v2::ServerMessage::SubscriptionError(ws::v2::SubscriptionError {
            request_id: Some(7),
            query_set_id: ws::common::QuerySetId::new(3),
            error: "example error ".repeat(400000).into(),
        });
        let bytes = spacetimedb_sats::bsatn::to_vec(&message).unwrap();
        for workers in [0, 2] {
            let frame = encode(&bytes, workers);
            assert_eq!(zstd_decompress_portable(&frame).unwrap(), bytes);
            let mut wire = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_ZSTD];
            wire.extend_from_slice(&frame);
            assert_eq!(decompress_server_message(&wire).unwrap().as_ref(), bytes);
            let decoded = crate::websocket::WsConnection::parse_response(&wire).unwrap();
            assert_eq!(spacetimedb_sats::bsatn::to_vec(&decoded).unwrap(), bytes);
        }
    }

    #[test]
    fn actual_server_frames_decode_in_the_sdk() {
        use spacetimedb::client::{
            messages::{serialize_v2, SerializeBuffer},
            ClientConfig, WsVersion,
        };
        use spacetimedb::subscription::row_list_builder_pool::BsatnRowListBuilderPool;

        let pool = BsatnRowListBuilderPool::new();
        for size in [32, 2048, 5 * 1024 * 1024] {
            for compression in [
                ws::common::Compression::None,
                ws::common::Compression::Brotli,
                ws::common::Compression::Gzip,
                ws::common::Compression::Zstd,
            ] {
                let message = ws::v2::ServerMessage::SubscriptionError(ws::v2::SubscriptionError {
                    request_id: Some(8),
                    query_set_id: ws::common::QuerySetId::new(4),
                    error: "a".repeat(size).into(),
                });
                let expected = spacetimedb_sats::bsatn::to_vec(&message).unwrap();
                let config = ClientConfig {
                    compression,
                    version: WsVersion::V2,
                    ..ClientConfig::for_test()
                };
                let (in_use, frame) = serialize_v2(&pool, SerializeBuffer::new(config), message, compression);
                let decoded = crate::websocket::WsConnection::parse_response(&frame).unwrap();
                assert_eq!(spacetimedb_sats::bsatn::to_vec(&decoded).unwrap(), expected);
                if frame[0] == ws::common::SERVER_MSG_COMPRESSION_TAG_ZSTD {
                    assert_eq!(zstd_decompress_portable(&frame[1..]).unwrap(), expected);
                }
                drop(frame);
                assert!(in_use.try_reclaim().is_some());
            }
        }
    }

    #[test]
    fn zstd_truncation_and_corruption_are_errors() {
        let frame = encode(&vec![42; 1024 * 1024], 0);
        for end in [0, 1, frame.len() / 2, frame.len() - 1] {
            assert!(zstd_decompress(&frame[..end]).is_err());
            assert!(zstd_decompress_portable(&frame[..end]).is_err());
        }
        let mut corrupted = frame;
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(zstd_decompress(&corrupted).is_err());
        assert!(zstd_decompress_portable(&corrupted).is_err());
        let mut wire = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_ZSTD];
        wire.extend_from_slice(&corrupted);
        assert!(matches!(
            decompress_server_message(&wire),
            Err(WsError::Decompress { scheme: "zstd", .. })
        ));
    }

    #[test]
    fn legacy_tags_and_uncompressed_frames_still_work() {
        assert!(matches!(decompress_server_message(&[]), Err(WsError::EmptyMessage)));
        assert!(matches!(
            decompress_server_message(&[255]),
            Err(WsError::UnknownCompressionScheme { .. })
        ));
        assert_eq!(decompress_server_message(&[0, 10, 20]).unwrap().as_ref(), &[10, 20]);
        let mut brotli = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_BROTLI];
        brotli::BrotliCompress(&mut &b"payload"[..], &mut brotli, &Default::default()).unwrap();
        assert_eq!(decompress_server_message(&brotli).unwrap().as_ref(), b"payload");
        let mut gzip = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_GZIP];
        let mut encoder = flate2::write::GzEncoder::new(&mut gzip, flate2::Compression::fast());
        encoder.write_all(b"payload").unwrap();
        encoder.finish().unwrap();
        assert_eq!(decompress_server_message(&gzip).unwrap().as_ref(), b"payload");
    }
}
