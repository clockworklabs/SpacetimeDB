/// List of supported platforms
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Linux,
    MacOs,
    /// Other Unix-like systems
    Other,
    Windows,
}

impl Platform {
    /// Get the current platform using the `cfg!(target_os = "...")` macro
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Other
        }
    }

    /// Check if the platform is Unix-like
    pub fn nix_like(&self) -> bool {
        match self {
            Platform::Linux | Platform::MacOs | Platform::Other => true,
            Platform::Windows => false,
        }
    }
}
