from .. import Smoketest

class WasmBindgen(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn test(_ctx: &ReducerContext) {
    log::info!("Hello! {}", now());
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    fn now() -> i32;
}
"""
    EXTRA_DEPS = 'wasm-bindgen = "0.2"'

    def test_detect_wasm_bindgen(self):
        """Ensure that spacetime build properly catches wasm_bindgen imports"""

        output = self.spacetime("build", "--project-path", self.project_path, full_output=True, check=False)
        self.assertTrue(output.returncode)
        self.assertIn("wasm-bindgen detected", output.stderr)

class Getrandom(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn test(_ctx: &ReducerContext) {
    log::info!("Hello! {}", rand::random::<u8>());
}
"""
    EXTRA_DEPS = 'rand = "0.8"'

    def test_detect_getrandom(self):
        """Ensure that spacetime build properly catches getrandom"""

        output = self.spacetime("build", "--project-path", self.project_path, full_output=True, check=False)
        self.assertTrue(output.returncode)
        self.assertIn("getrandom usage detected", output.stderr)
