from .. import Smoketest, extract_field

class ModuleIdentity(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn print_module_identity(ctx: &ReducerContext) {
    log::info!("Module identity: {}", ctx.identity());
}
"""

    def test_module_identity(self):
        """Check using `ctx.identity()` to read the module identity."""

        self.call("print_module_identity")

        self.assertIn(f"Module identity: {self.database_identity}", self.logs(1))
