use tokio::runtime::Builder;
use tokio::fs;
use std::{error::Error, sync::Arc};
use wasmer::{Store, Module, Instance, Value, imports, wasmparser::Operator, Cranelift, CompilerConfig, wat2wasm, Universal, Function};
use wasmer_middlewares::{
    metering::get_remaining_points,
    Metering,
};
use wasmer::Extern;

async fn async_main() -> Result<(), Box<dyn Error>> {
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/wasm-test/build/debug.wat")).await.unwrap();
    let wat = fs::read(path).await?;
    println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes= wat2wasm(&wat)?;

    let cost_function = |operator: &Operator| -> u64 {
        match operator {
            Operator::LocalGet { .. } => 1,
            Operator::I32Const { .. } => 1,
            Operator::I32Add { .. } => 5,
            _ => 0,
        }
    };

    let metering = Arc::new(Metering::new(10, cost_function));
    let mut compiler_config = Cranelift::default();
    compiler_config.push_middleware(metering);

    fn test() -> i32 {
        67
    }

    let store = Store::new(&Universal::new(compiler_config).engine());
    let module = Module::new(&store, wasm_bytes)?;
    let import_object = imports! {
        "stdb" => {
            "test" => Function::new_native(&store, test),
        },
    };

    let instance = Instance::new(&module, &import_object)?;

    for (name, value) in instance.exports.iter() {
        match value {
            Extern::Function(f) => {},
            Extern::Global(_g) => {},
            Extern::Table(_t) => {},
            Extern::Memory(_m) => {},
        }
    }

    let reduce = instance.exports.get_function("reduce")?.native::<u64, i32>()?;

    let x = reduce.call(34234)?;
    let remaining_points_after_first_call = get_remaining_points(&instance);
    println!("Remaining points {:?}", remaining_points_after_first_call);
    println!("x: {}", x);

    Ok(())
}

fn main() {
    // Create a single threaded run loop
    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}