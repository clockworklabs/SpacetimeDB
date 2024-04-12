use spacetimedb_client_api_messages::ws::{ClientMessage, ServerMessage};
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::ModuleDef;

fn main() -> Result<(), serde_json::Error> {
    let module = ModuleDef::with_builder(|module| {
        module.add_type::<ClientMessage>();
        module.add_type::<ServerMessage>();
    });

    serde_json::to_writer(std::io::stdout().lock(), SerializeWrapper::from_ref(&module))
}
