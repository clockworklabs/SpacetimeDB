from .. import Smoketest

class WasmBindgen(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::{log, spacetimedb};

#[spacetimedb(reducer)]
pub fn test() {
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
use spacetimedb::{log, spacetimedb};

#[spacetimedb(reducer)]
pub fn test() {
    log::info!("Hello! {}", rand::random::<u8>());
}
"""
    EXTRA_DEPS = 'rand = "0.8"'

    def test_detect_getrandom(self):
        """Ensure that spacetime build properly catches getrandom"""

        output = self.spacetime("build", "--project-path", self.project_path, full_output=True, check=False)
        self.assertTrue(output.returncode)
        self.assertIn("getrandom usage detected", output.stderr)

class Stdio(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE_TMPL = """
use spacetimedb::spacetimedb;

#[spacetimedb(reducer)]
pub fn test() {{
    {}!("hello!");
}}
"""
    EXTRA_DEPS = 'rand = "0.8"'

    def do_test(self, macro):
        self.write_module_code(self.MODULE_CODE_TMPL.format(macro))
        output = self.spacetime("build", "--project-path", self.project_path, full_output=True)
        self.assertIn("stdio usage detected", output.stderr)


    def test_detect_println(self):
        """Ensure that spacetime build properly catches println!()"""
        self.do_test("println")


    def test_detect_eprintln(self):
        """Ensure that spacetime build properly catches eprintln!()"""
        self.do_test("eprintln")


    def test_detect_dbg(self):
        """Ensure that spacetime build properly catches dbg!()"""
        self.do_test("dbg")
