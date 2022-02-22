pub mod content_addressed_table;
pub mod diff_table;
pub mod object_db;
pub mod hash;

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};
    use wasmer::{Store, Module, Instance, imports, wasmparser::Operator, Cranelift, CompilerConfig, wat2wasm, Universal, Function};
    use wasmer_middlewares::{
        metering::{get_remaining_points, MeteringPoints},
        Metering,
    };
    static mut HEALTH: i32 = 10;

    #[test]
    fn main() -> Result<(), Box<dyn Error>> {
        let wasm_bytes= wat2wasm(br#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32 i32)))
            (type (;2;) (func (param i32) (result i32)))
            (import "stdb" "get_health" (func (;0;) (type 2)))
            (import "stdb" "set_health" (func (;1;) (type 1)))
            (func (;2;) (export "reduce") (type 0)
              (call 1
                (i32.const 1)
                (i32.add
                  (call 0
                    (i32.const 1))
                  (i32.const 4))))
            (memory (;0;) (export "memory") 17)
            (data (;0;) (i32.const 1048576) "\04"))
        "#)?;

        fn get_health(_entity_id: i64) -> i32 {
            unsafe { HEALTH }
        }
        
        fn set_health(_entity_id: i64, health: i32) {
            unsafe { HEALTH = health }
        }

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

        let store = Store::new(&Universal::new(compiler_config).engine());
        let module = Module::new(&store, wasm_bytes)?;
        let import_object = imports! {
            "stdb" => {
                "get_health" => Function::new_native(&store, get_health),
                "set_health" => Function::new_native(&store, set_health)
            },
        };

        let instance = Instance::new(&module, &import_object)?;

        let reduce = instance.exports.get_function("reduce")?.native::<(), ()>()?;

        reduce.call()?;
        let remaining_points_after_first_call = get_remaining_points(&instance);
        println!("Remaining points {:?}", remaining_points_after_first_call);
        println!("{}", unsafe { HEALTH });

        match reduce.call() {
            Ok(_) => panic!("Should have enough gas."),
            Err(_) => {
                let remaining_points = get_remaining_points(&instance);
                assert_eq!(MeteringPoints::Exhausted, remaining_points);
            }
        };

        Ok(())
    }
}