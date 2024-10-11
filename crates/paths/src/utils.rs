use std::path::{Path, PathBuf};

pub(crate) trait PathBufExt {
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

#[macro_export]
macro_rules! path_type {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Clone, Debug, $crate::__serde::Serialize, $crate::__serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub std::path::PathBuf);

        impl AsRef<std::path::Path> for $name {
            fn as_ref(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl From<std::ffi::OsString> for $name {
            fn from(s: std::ffi::OsString) -> Self {
                Self(s.into())
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
    ($(#[$attr:meta])* $name:ident: dir) => {
        path_type!($(#[$attr])* $name);
        impl $name {
            #[inline]
            pub fn create(&self) -> std::io::Result<()> {
                std::fs::create_dir_all(self)
            }
            #[inline]
            pub fn read_dir(&self) -> std::io::Result<std::fs::ReadDir> {
                self.0.read_dir()
            }
            #[inline]
            pub fn is_dir(&self) -> bool {
                self.0.is_dir()
            }
        }
    };
    ($(#[$attr:meta])* $name:ident: file) => {
        path_type!($(#[$attr])* $name);
        impl $name {
            pub fn read(&self) -> std::io::Result<Vec<u8>> {
                std::fs::read(self)
            }

            pub fn read_to_string(&self) -> std::io::Result<String> {
                std::fs::read_to_string(self)
            }

            pub fn write(&self, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
                self.create_parent()?;
                std::fs::write(self, contents)
            }

            /// Opens a file at this path with the given options, ensuring its parent directory exists.
            #[inline]
            pub fn open_file(&self, options: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
                self.create_parent()?;
                options.open(self)
            }

            /// Create the parent directory of this path if it doesn't already exist.
            #[inline]
            pub fn create_parent(&self) -> std::io::Result<()> {
                if let Some(parent) = self.0.parent() {
                    if parent != std::path::Path::new("") {
                        std::fs::create_dir_all(parent)?;
                    }
                }
                Ok(())
            }
        }
    };
}
pub(crate) use path_type;
