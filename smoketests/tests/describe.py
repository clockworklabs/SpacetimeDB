from .. import Smoketest

class ModuleDescription(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, ReducerContext, Table};

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
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
"""

    def test_describe(self):
        """Check describing a module"""

        self.spacetime("describe", self.database_identity)
        self.spacetime("describe", self.database_identity, "reducer", "say_hello")
        self.spacetime("describe", self.database_identity, "table", "person")
