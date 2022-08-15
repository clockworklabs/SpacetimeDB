use tokio::io::AsyncReadExt;

use crate::hash::Hash;
use std::fs::{self, File};
use std::fs::OpenOptions;
use std::io::{prelude::*, SeekFrom};
use std::path::{PathBuf, Path};
use std::str::FromStr;

pub struct DatabaseLogger {
    filepath: PathBuf,
    file: File
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

    pub fn open(root: impl AsRef<Path>, filename: &str) -> Self {
        let root = root.as_ref();
        fs::create_dir_all(root).unwrap();

        let mut filepath = PathBuf::from(root);
        filepath.push(&PathBuf::from_str(&format!("{}", filename)).unwrap());

        let file = OpenOptions::new().create(true).append(true).open(&filepath).unwrap();
        Self {
            file,
            filepath,
        }
    }

    pub fn delete(&mut self) {
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

    pub async fn _read_all(&self) -> String {
        use tokio::fs;
        let contents = String::from_utf8(fs::read(&self.filepath).await.unwrap()).unwrap();
        contents
    }

    pub async fn read_latest(&self, num_lines: u32) -> String {
        // TODO: Read backwards from the end of the file to only read in the latest lines
        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&self.filepath)
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

}


