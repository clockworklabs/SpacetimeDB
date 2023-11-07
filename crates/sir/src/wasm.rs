use wasmtime::*;

pub struct Wasm {
    engine: Engine,
    store: Store<()>,
}

impl Wasm {
    pub fn new() -> Self {
        let engine = Engine::default();
        let engine_clone = engine.clone();

        Self {
            engine,
            store: Store::new(&engine_clone, ()),
        }
    }
}
