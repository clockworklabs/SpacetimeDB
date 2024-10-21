use crate::server::ServerDataPath;

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

impl sealed::Sealed for ServerDataPath {}
impl StandaloneDataDirExt for ServerDataPath {}

path_type!(ProgramBytesDir: dir);
path_type!(ControlDbDir: dir);
