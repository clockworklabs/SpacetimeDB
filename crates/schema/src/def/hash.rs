use std::hash::Hash;

use spacetimedb_lib::bsatn;
use spacetimedb_lib::buffer::HashWriter;
use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10;

use crate::def::ModuleDef;

impl Hash for ModuleDef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let raw = RawModuleDefV10::from(self.clone());
        bsatn::to_writer(HashWriter::from_mut(state), &raw).unwrap();
    }
}
