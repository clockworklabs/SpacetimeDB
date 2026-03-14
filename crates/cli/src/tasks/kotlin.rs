use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;

fn has_ktfmt() -> bool {
    duct::cmd!("ktfmt", "--version")
        .stdout_null()
        .stderr_null()
        .run()
        .is_ok()
}

pub(crate) fn ktfmt(files: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<()> {
    if !has_ktfmt() {
        eprintln!("ktfmt not found — skipping Kotlin formatting.");
        eprintln!("Install ktfmt from https://github.com/facebook/ktfmt to auto-format generated code.");
        return Ok(());
    }
    duct::cmd(
        "ktfmt",
        itertools::chain(
            ["--kotlinlang-style"].into_iter().map_into::<OsString>(),
            files.into_iter().map_into(),
        ),
    )
    .run()
    .context("ktfmt failed")?;
    Ok(())
}
