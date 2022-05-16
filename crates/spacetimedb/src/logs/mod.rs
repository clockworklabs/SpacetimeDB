use crate::hash::Hash;
use std::fs;
use std::fs::OpenOptions;
use std::io::prelude::*;

const ROOT: &str = "/stdb/logs";

fn path_from_address(module_address: Hash) -> String {
    let hex_address = hex::encode(module_address);
    let path = format!("{}/{}/{}.log", ROOT, &hex_address[0..2], &hex_address[2..]);
    path
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

pub fn write(module_address: Hash, _level: u8, value: String) {
    let path = path_from_address(module_address);
    println!("Path: {}", path);
    let mut file = OpenOptions::new().append(true).write(true).open(path).unwrap();

    // match level {
    //     0 => eprintln!("error: {}", s),
    //     1 => println!("warn: {}", s),
    //     2 => println!("info: {}", s),
    //     3 => println!("debug: {}", s),
    //     _ => println!("debug: {}", s),
    // }

    writeln!(file, "{}", value).unwrap();
}
