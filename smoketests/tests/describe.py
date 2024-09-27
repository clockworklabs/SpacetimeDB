from .. import Smoketest

class ModuleDescription(Smoketest):
    MODULE_CODE = """
use spacetimedb::println;

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
"""

    def test_describe(self):
        """Check describing a module"""

        self.spacetime("describe", self.address)
        self.spacetime("describe", self.address, "-t", "reducer", "say_hello")
        self.spacetime("describe", self.address, "-t", "table", "person")
