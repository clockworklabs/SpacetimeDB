use crate::server::ServerDataDir;
use crate::utils::path_type;

pub trait StandaloneDataDirExt: AsRef<std::path::Path> {
    fn program_bytes(&self) -> ProgramBytesDir {
        ProgramBytesDir(self.as_ref().join("program-bytes"))
    }
    fn control_db(&self) -> ControlDbDir {
        ControlDbDir(self.as_ref().join("control-db"))
    }
}

impl StandaloneDataDirExt for ServerDataDir {}

path_type!(ProgramBytesDir: dir);
path_type!(ControlDbDir: dir);
