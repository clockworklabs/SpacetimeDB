use std::borrow::BorrowMut;
use std::path::{Path, PathBuf};

pub(crate) trait PathBufExt: BorrowMut<PathBuf> + Sized {
    fn joined<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.borrow_mut().push(path);
        self
    }
    fn with_exe_ext(mut self) -> Self {
        self.borrow_mut().set_extension(std::env::consts::EXE_EXTENSION);
        self
    }
    fn joined_int<I: itoa::Integer>(self, path_seg: I) -> Self {
        self.joined(itoa::Buffer::new().format(path_seg))
    }
}

impl PathBufExt for PathBuf {}

/// Declares a new strongly-typed path newtype.
///
/// ```
/// # use spacetimedb_paths::path_type;
/// path_type! {
///     /// optional docs
///     // optional. if false, makes the type's constructor public.
/// #   // TODO: replace cfg(any()) with cfg(false) once stabilized (rust-lang/rust#131204)
///     #[non_exhaustive(any())]
///     FooPath: dir // or file. adds extra utility methods for manipulating the file/dir
/// }
/// ```
#[macro_export]
macro_rules! path_type {
    ($(#[doc = $doc:literal])* $(#[non_exhaustive($($non_exhaustive:tt)+)])? $name:ident) => {
        $(#[doc = $doc])*
        #[derive(Clone, Debug, $crate::__serde::Serialize, $crate::__serde::Deserialize)]
        #[serde(transparent)]
        #[cfg_attr(all($($($non_exhaustive)+)?), non_exhaustive)]
        pub struct $name(pub std::path::PathBuf);

        impl AsRef<std::path::Path> for $name {
            #[inline]
            fn as_ref(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl AsRef<std::ffi::OsStr> for $name {
            #[inline]
            fn as_ref(&self) -> &std::ffi::OsStr {
                self.0.as_ref()
            }
        }

        impl From<std::ffi::OsString> for $name {
            fn from(s: std::ffi::OsString) -> Self {
                Self(s.into())
            }
        }

        impl $crate::FromPathUnchecked for $name {
            fn from_path_unchecked(path: impl Into<std::path::PathBuf>) -> Self {
                Self(path.into())
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
    ($(#[$($attr:tt)+])* $name:ident: dir) => {
        path_type!($(#[$($attr)+])* $name);
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
    ($(#[$($attr:tt)+])* $name:ident: file) => {
        path_type!($(#[$($attr)+])* $name);
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
