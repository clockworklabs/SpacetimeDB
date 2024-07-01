use std::{
    fs::{create_dir_all, File},
    io::{self, Read, Write},
    iter,
    path::{Path, PathBuf},
};

use proptest::prelude::*;
use spacetimedb_commitlog::dio::{open_file, PagedReader, PagedWriter};
use tempfile::NamedTempFile;

#[test]
fn smoke() {
    let path = tempdir().join("smoke");

    let input: &[u8] = &[42; 5120];
    let output = roundtrip(&path, Some(input)).unwrap();

    assert_eq!(input.len(), output.len());
    assert_eq!(input, &output);
}

#[test]
fn small_writes() {
    let path = tempdir().join("small-writes");

    let input: &[&[u8]] = &[b"guten tag\n", b"wie geht's\n", b"s'klar\n", b"man sieht sich\n"];
    let output = roundtrip(&path, input.iter().cloned()).unwrap();

    let mut input = input.concat();
    input.extend(iter::repeat(0).take(512 - input.len()));

    assert_eq!(input, output);
}

fn gen_input() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(any::<u8>(), 1..=9216), 1..500)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        source_file: Some("dio"),
        ..ProptestConfig::default()
    })]
    #[test]
    fn mixed_writes(input in gen_input()) {
        let tmp = NamedTempFile::new_in(tempdir()).unwrap();
        let output = roundtrip(tmp.path(), input.iter().map(Vec::as_slice)).unwrap();

        let mut input = input.concat();
        input.extend(iter::repeat(0).take(input.len().next_multiple_of(512) - input.len()));

        assert_eq!(input, output);
    }
}

fn tempdir() -> PathBuf {
    let mut path = super::tempdir().to_path_buf();
    path.push("commitlog");
    path.push("dio");
    create_dir_all(&path).unwrap();
    path
}

fn writer(path: &Path) -> io::Result<PagedWriter<File>> {
    open_file(path, File::options().create(true).write(true).truncate(true)).map(PagedWriter::new)
}

fn reader(path: &Path) -> io::Result<PagedReader<File>> {
    open_file(path, File::options().read(true)).map(PagedReader::new)
}

fn roundtrip<'a>(path: &Path, input: impl IntoIterator<Item = &'a [u8]>) -> io::Result<Vec<u8>> {
    let mut writer = writer(path)?;
    for chunk in input {
        writer.write_all(chunk)?;
    }
    writer.sync_data()?;

    let mut reader = reader(path)?;
    let mut ret = Vec::with_capacity(2 * 4096);
    let mut buf = [0; 4096];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ret.extend_from_slice(&buf[..n]);
    }

    Ok(ret)
}
