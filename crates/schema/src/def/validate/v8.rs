use crate::def::validate::Result;
use crate::def::*;
use spacetimedb_lib::db::raw_def::v8::*;

/// Validate a `RawModuleDefV8` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(_def: RawModuleDefV8) -> Result<ModuleDef> {
    unimplemented!()
}
