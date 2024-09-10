use spacetimedb_sdk::{
    spacetime_module::InModule,
    spacetimedb_lib::{de::Deserialize, ser::Serialize},
    Identity,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct User {
    pub identity: Identity,
    pub name: Option<String>,
    pub online: bool,
}

impl InModule for User {
    type Module = super::RemoteModule;
}
