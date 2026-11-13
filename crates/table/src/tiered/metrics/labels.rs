#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType {
    Page,
    Blob,
}

impl ObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Blob => "blob",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheTier {
    Memory,
    Disk,
}

impl CacheTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Disk => "disk",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultSource {
    Disk,
    Remote,
}

impl FaultSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disk => "disk",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvictionDirection {
    MemoryToDisk,
    DiskToRemote,
}

impl EvictionDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MemoryToDisk => "memory_to_disk",
            Self::DiskToRemote => "disk_to_remote",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskOperation {
    Read,
    Write,
}

impl DiskOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}
