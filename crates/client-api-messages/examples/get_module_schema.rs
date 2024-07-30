use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::RawModuleDefV8;

fn main() -> Result<(), serde_json::Error> {
    let module = RawModuleDefV8::with_builder(|module| {
        module.add_type::<RawModuleDefV8>();
    });

    serde_json::to_writer(std::io::stdout().lock(), SerializeWrapper::from_ref(&module))
}
