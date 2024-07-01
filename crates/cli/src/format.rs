use clap::builder::PossibleValue;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

/// Output format for tabular data.
///
/// Implements [`clap::ValueEnum`].
#[derive(Clone, Copy, Debug)]
pub enum OutputFormat {
    Json,
    Table,
    Csv,
}

impl clap::ValueEnum for OutputFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Json, Self::Table, Self::Csv]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::Json => PossibleValue::new("json").help("Render output as JSON"),
            Self::Table => PossibleValue::new("table").help("Render output in an ASCII table format"),
            Self::Csv => PossibleValue::new("csv").help("Render output as CSV"),
        })
    }
}

/// Write the given `value` to `out` in the [RFC7464] 'json-seq' format.
///
/// This format is understood by `jq --seq`, which will decode each object from
/// the input in a streaming fashion.
///
/// Note: this function does not flush the output writer.
///
/// [RFC7464]: https://datatracker.ietf.org/doc/html/rfc7464
pub async fn write_json_seq<W: AsyncWrite + Unpin, T: serde::Serialize>(mut out: W, value: &T) -> anyhow::Result<()> {
    let rendered = serde_json::to_vec(value)?;
    out.write_u8(0x1E).await?;
    out.write_all(&rendered).await?;
    out.write_u8(b'\n').await?;

    Ok(())
}
