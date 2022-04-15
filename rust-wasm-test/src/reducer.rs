use wasm_bindgen::prelude::*;

mod stdb {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(raw_module = "stdb")]
    extern "C" {
        pub fn set_health(entity_id: i32, health: i32);
        pub fn get_health(entity_id: i32) -> i32;
    }
}

#[wasm_bindgen]
pub fn reduce() {
    let health = stdb::get_health(1);
    stdb::set_health(1, health + 4);
}