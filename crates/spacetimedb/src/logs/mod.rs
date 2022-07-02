use tokio::io::AsyncReadExt;

use crate::hash::Hash;
use std::fs;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::PathBuf;
use std::str::FromStr;

const ROOT: &str = "/stdb/logs";

fn path_from_hash(hash: Hash) -> PathBuf {
    let hex_address = hex::encode(hash);
    let path = format!("{}/{}", &hex_address[0..2], &hex_address[2..]);
    PathBuf::from(path)
}

fn log_dir_from(identity: Hash, _name: &str) -> PathBuf {
    let mut path = PathBuf::from(ROOT);
    path.push(path_from_hash(identity));
    path
}

fn log_path_from(identity: Hash, name: &str) -> PathBuf {
    let mut path = log_dir_from(identity, name);
    path.push(PathBuf::from_str(&format!("{}.log", name)).unwrap());
    path
}

pub fn init_log(identity: Hash, name: &str) {
    let dir = log_dir_from(identity, name);
    let path = log_path_from(identity, name);
    fs::create_dir_all(dir).unwrap();
    OpenOptions::new().create(true).write(true).open(path).unwrap();
}

pub fn delete_log(identity: Hash, name: &str) {
    let dir = log_dir_from(identity, name);
    fs::remove_dir_all(dir).unwrap();
}

pub fn write(identity: Hash, name: &str, level: u8, value: String) {
    let dir = log_dir_from(identity, name);
    fs::create_dir_all(dir).unwrap();

    let path = log_path_from(identity, name);
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

pub async fn _read_all(identity: Hash, name: &str) -> String {
    use tokio::fs;
    let path = log_path_from(identity, name);
    let contents = String::from_utf8(fs::read(path).await.unwrap()).unwrap();
    contents
}

pub async fn read_latest(identity: Hash, name: &str, num_lines: u32) -> String {
    // let file = fs::File::open(path).await.unwrap();
    // let reader = BufReader::new(file);
    // while let Some(line) = reader.lines().next_line().await.unwrap() {
    // }

    let path = log_path_from(identity, name);
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
