use spacetimedb_sdk::{
    spacetime_module::InModule,
    spacetimedb_lib::{de::Deserialize, ser::Serialize},
    Identity,
};
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Message {
    pub sender: Identity,
    pub sent: SystemTime,
    pub text: String,
}

impl InModule for Message {
    type Module = super::RemoteModule;
}
