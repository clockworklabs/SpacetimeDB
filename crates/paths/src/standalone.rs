use crate::server::ServerDataDir;
use crate::utils::path_type;

mod sealed {
    pub trait Sealed: AsRef<std::path::Path> {}
}

pub trait StandaloneDataDirExt: sealed::Sealed {
    fn program_bytes(&self) -> ProgramBytesDir {
        ProgramBytesDir(self.as_ref().join("program-bytes"))
    }
    fn control_db(&self) -> ControlDbDir {
        ControlDbDir(self.as_ref().join("control-db"))
    }
}

impl sealed::Sealed for ServerDataDir {}
impl StandaloneDataDirExt for ServerDataDir {}

path_type!(ProgramBytesDir: dir);
path_type!(ControlDbDir: dir);
