use spacetimedb_client_api_messages::websocket::{ClientMessage, ServerMessage};
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::RawModuleDefV0;

fn main() -> Result<(), serde_json::Error> {
    let module = RawModuleDefV0::with_builder(|module| {
        module.add_type::<ClientMessage>();
        module.add_type::<ServerMessage>();
    });

    serde_json::to_writer(std::io::stdout().lock(), SerializeWrapper::from_ref(&module))
}
