use std::path::{Path, PathBuf};

trait PathBufExt {
    fn joined<P: AsRef<Path>>(self, path: P) -> Self;
    fn joined_int<I: itoa::Integer>(self, path_seg: I) -> Self;
}

impl PathBufExt for PathBuf {
    fn joined<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.push(path);
        self
    }

    fn joined_int<I: itoa::Integer>(self, path_seg: I) -> Self {
        self.joined(itoa::Buffer::new().format(path_seg))
    }
}

macro_rules! path_type {
    ($name:ident) => {
        #[derive(Clone, Debug)]
        pub struct $name(pub std::path::PathBuf);

        impl AsRef<std::path::Path> for $name {
            fn as_ref(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl $name {
            #[inline]
            pub fn display(&self) -> std::path::Display<'_> {
                self.0.display()
            }

            #[inline]
            pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
                self.0.metadata()
            }
        }
    };
    ($name:ident: dir) => {
        path_type!($name);
        impl $name {
            #[inline]
            pub fn create(&self) -> std::io::Result<()> {
                std::fs::create_dir_all(self)
            }
            #[inline]
            pub fn read_dir(&self) -> std::io::Result<std::fs::ReadDir> {
                self.0.read_dir()
            }
        }
    };
    ($name:ident: file) => {
        path_type!($name);
        impl $name {
            /// Opens a file at this path with the given options, ensuring its parent directory exists.
            #[inline]
            pub fn open_file(&self, options: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
                if let Some(parent) = self.0.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                options.open(self)
            }
        }
    };
}

pub mod server;
pub mod standalone;
