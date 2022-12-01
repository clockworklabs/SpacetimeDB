use crate::address::Address;
use std::cmp::min;
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{prelude::*, SeekFrom};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::io::AsyncReadExt;

pub struct DatabaseLogger {
    file: File,
}

impl DatabaseLogger {
    // fn log_dir_from(identity: Hash, _name: &str) -> PathBuf {
    //     let mut path = PathBuf::from(ROOT);
    //     path.push(Self::path_from_hash(identity));
    //     path
    // }

    // fn log_path_from(identity: Hash, name: &str) -> PathBuf {
    //     let mut path = Self::log_dir_from(identity, name);
    //     path.push(PathBuf::from_str(&format!("{}.log", name)).unwrap());
    //     path
    // }

    // fn path_from_hash(hash: Hash) -> PathBuf {
    //     let hex_address = hash.to_hex();
    //     let path = format!("{}/{}", &hex_address[0..2], &hex_address[2..]);
    //     PathBuf::from(path)
    // }

    pub fn filepath(address: &Address, instance_id: u64) -> String {
        let root = "/stdb/worker_node/database_instances";
        format!("{}/{}/{}/{}", root, address.to_hex(), instance_id, "module_logs")
    }

    pub fn open(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        fs::create_dir_all(root).unwrap();

        let mut filepath = PathBuf::from(root);
        filepath.push(&PathBuf::from_str("0.log").unwrap());

        let file = OpenOptions::new().create(true).append(true).open(&filepath).unwrap();
        Self { file }
    }

    pub fn _delete(&mut self) {
        self.file.set_len(0).unwrap();
        self.file.seek(SeekFrom::End(0)).unwrap();
    }

    pub fn write(&mut self, level: u8, value: String) {
        let file = &mut self.file;
        match level {
            0 => writeln!(file, "error: {}", value).unwrap(),
            1 => writeln!(file, " warn: {}", value).unwrap(),
            2 => writeln!(file, " info: {}", value).unwrap(),
            3 => writeln!(file, "debug: {}", value).unwrap(),
            _ => writeln!(file, "debug: {}", value).unwrap(),
        }
    }

    pub async fn _read_all(root: &str) -> String {
        let mut filepath = PathBuf::from(root);
        filepath.push(&PathBuf::from_str("0.log").unwrap());

        use tokio::fs;
        //contents
        String::from_utf8(fs::read(filepath).await.unwrap()).unwrap()
    }

    pub async fn read_latest(root: &str, num_lines: Option<u32>) -> String {
        let mut filepath = PathBuf::from(root);
        filepath.push(&PathBuf::from_str("0.log").unwrap());

        // TODO: Read backwards from the end of the file to only read in the latest lines
        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(filepath)
            .await
            .expect("opening file");

        let mut text = String::new();
        file.read_to_string(&mut text).await.expect("reading file");

        let lines: Vec<&str> = text.lines().collect();
        let num_lines: usize = match num_lines {
            None => lines.len(),
            Some(val) => min(val as usize, lines.len()),
        };

        let start = lines.len() - num_lines;
        let end = lines.len();
        let latest = &lines[start..end];

        latest.join("\n")
    }
}
