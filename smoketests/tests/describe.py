from .. import Smoketest

class ModuleDescription(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"""

    def test_describe(self):
        """Check describing a module"""

        self.spacetime("describe", "--json", self.database_identity)
        self.spacetime("describe", "--json", self.database_identity, "reducer", "say_hello")
        self.spacetime("describe", "--json", self.database_identity, "table", "person")
