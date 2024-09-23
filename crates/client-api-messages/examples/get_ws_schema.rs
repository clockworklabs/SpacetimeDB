use spacetimedb_client_api_messages::websocket::{ClientMessage, ServerMessage};
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::{RawModuleDef, RawModuleDefV8};

fn main() -> Result<(), serde_json::Error> {
    let module = RawModuleDefV8::with_builder(|module| {
        module.add_type::<ClientMessage>();
        module.add_type::<ServerMessage>();
    });
    let module = RawModuleDef::V8BackCompat(module);

    serde_json::to_writer(std::io::stdout().lock(), SerializeWrapper::from_ref(&module))
}
