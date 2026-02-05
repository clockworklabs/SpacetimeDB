use crate::websocket::WsError;
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_sats::bsatn;
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

pub(crate) fn maybe_decompress_cqu(
    cqu: ws::v1::CompressableQueryUpdate<ws::v1::BsatnFormat>,
) -> ws::v1::QueryUpdate<ws::v1::BsatnFormat> {
    match cqu {
        ws::v1::CompressableQueryUpdate::Uncompressed(qu) => qu,
        ws::v1::CompressableQueryUpdate::Brotli(bytes) => {
            let bytes = brotli_decompress(&bytes).unwrap();
            bsatn::from_slice(&bytes).unwrap()
        }
        ws::v1::CompressableQueryUpdate::Gzip(bytes) => {
            let bytes = gzip_decompress(&bytes).unwrap();
            bsatn::from_slice(&bytes).unwrap()
        }
    }
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
        [ws::v1::SERVER_MSG_COMPRESSION_TAG_NONE, bytes @ ..] => Ok(Cow::Borrowed(bytes)),
        [ws::v1::SERVER_MSG_COMPRESSION_TAG_BROTLI, bytes @ ..] => brotli_decompress(bytes)
            .map(Cow::Owned)
            .map_err(err_decompress("brotli")),
        [ws::v1::SERVER_MSG_COMPRESSION_TAG_GZIP, bytes @ ..] => {
            gzip_decompress(bytes).map(Cow::Owned).map_err(err_decompress("gzip"))
        }
        [c, ..] => Err(WsError::UnknownCompressionScheme { scheme: *c }),
    }
}
