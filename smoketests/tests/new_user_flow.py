from .. import Smoketest
import time

class NewUserFlow(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String
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

    def test_new_user_flow(self):
        """Test the entirety of the new user flow."""

        ## Publish your module
        self.new_identity()

        self.publish_module()

        # Calling our database
        self.call("say_hello")
        self.assertIn("Hello, World!", self.logs(2))

        ## Calling functions with arguments
        self.call("add", "Tyler")
        self.call("say_hello")
        self.assertEqual(self.logs(5).count("Hello, World!"), 2)
        self.assertEqual(self.logs(5).count("Hello, Tyler!"), 1)

        out = self.spacetime("sql", self.database_identity, "SELECT * FROM person")
        # The spaces after the name are important
        self.assertMultiLineEqual(out, """\
 name    
---------
 "Tyler" 
""")
