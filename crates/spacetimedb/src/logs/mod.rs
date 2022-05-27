use tokio::io::AsyncReadExt;

use crate::hash::Hash;
use std::fs;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::PathBuf;

const ROOT: &str = "/stdb/logs";

fn path_from_address(module_address: Hash) -> PathBuf {
    let hex_address = hex::encode(module_address);
    let path = format!("{}/{}/{}.log", ROOT, &hex_address[0..2], &hex_address[2..]);
    PathBuf::from(path)
}

fn dir_from_address(module_address: Hash) -> String {
    let hex_address = hex::encode(module_address);
    let path = format!("{}/{}", ROOT, &hex_address[0..2]);
    path
}

pub fn init_log(module_address: Hash) {
    let path = path_from_address(module_address);
    let dir = dir_from_address(module_address);
    fs::create_dir_all(dir).unwrap();
    OpenOptions::new().create(true).write(true).open(path).unwrap();
}

pub fn write(module_address: Hash, level: u8, value: String) {
    let path = path_from_address(module_address);
    let parent_dir = path.parent().unwrap();
    fs::create_dir_all(parent_dir).unwrap();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(path)
        .unwrap();

    match level {
        0 => writeln!(file, "error: {}", value).unwrap(),
        1 => writeln!(file, " warn: {}", value).unwrap(),
        2 => writeln!(file, " info: {}", value).unwrap(),
        3 => writeln!(file, "debug: {}", value).unwrap(),
        _ => writeln!(file, "debug: {}", value).unwrap(),
    }
}

pub async fn _read_all(module_address: Hash) -> String {
    use tokio::fs;
    let path = path_from_address(module_address);
    let contents = String::from_utf8(fs::read(path).await.unwrap()).unwrap();
    contents
}

pub async fn read_latest(module_address: Hash, num_lines: u32) -> String {
    // let file = fs::File::open(path).await.unwrap();
    // let reader = BufReader::new(file);
    // while let Some(line) = reader.lines().next_line().await.unwrap() {
    // }

    let path = path_from_address(module_address);
    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .await
        .expect("opening file");
    let mut text = String::new();
    file.read_to_string(&mut text).await.expect("reading file");

    let lines: Vec<&str> = text.lines().collect();

    let start = if lines.len() <= num_lines as usize {
        0_usize
    } else {
        lines.len() - num_lines as usize
    };
    let end = lines.len();
    let latest = &lines[start..end];

    latest.join("\n")
}
