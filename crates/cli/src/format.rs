use spacetimedb_lib::{
    sats::{satn, WithTypespace},
    ProductType, ProductValue,
};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

/// Output format for tabular data.
///
/// Implements [`clap::ValueEnum`].
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    /// Render output as JSON
    Json,
    /// Render output in an ASCII table format
    Table,
    /// Render output as CSV
    Csv,
}

/// A pre-configured [`clap::Arg`] for [`OutputFormat`].
pub fn arg_output_format(default_format: &'static str) -> clap::Arg {
    clap::Arg::new("output-format")
        .long("output-format")
        .short('o')
        .value_parser(clap::value_parser!(OutputFormat))
        .default_value(default_format)
        .help("How to format tabular data.")
}

/// Get the [`OutputFormat`] arg as configured using [`arg_output_format`].
pub fn get_arg_output_format(args: &clap::ArgMatches) -> OutputFormat {
    args.get_one("output-format").copied().unwrap()
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

/// Types which can be rendered according to an [`OutputFormat`].
pub trait Render: Sized {
    /// Render to `out` as JSON.
    async fn render_json(self, out: impl AsyncWrite + Unpin) -> anyhow::Result<()>;
    /// Render to `out` as ASCII table(s).
    async fn render_tabled(self, out: impl AsyncWrite + Unpin) -> anyhow::Result<()>;
    /// Render to `out` as CSV.
    async fn render_csv(self, out: impl AsyncWrite + Unpin) -> anyhow::Result<()>;
}

/// Render the given [`Render`]-able to `out` using `fmt`.
pub async fn render(r: impl Render, fmt: OutputFormat, out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
    use OutputFormat::*;

    match fmt {
        Json => r.render_json(out).await,
        Table => r.render_tabled(out).await,
        Csv => r.render_csv(out).await,
    }
}

/// Format each field in `row` as a string using psql formatting / escaping rules.
pub(crate) fn fmt_row_psql<'a>(
    row: &'a ProductValue,
    schema: WithTypespace<'a, ProductType>,
) -> impl Iterator<Item = String> + 'a {
    schema
        .with_values(row)
        .map(move |value| satn::PsqlWrapper { ty: schema.ty(), value }.to_string())
}
