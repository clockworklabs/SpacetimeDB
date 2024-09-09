from .. import Smoketest
import time

class NewUserFlow(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::println;

#[spacetimedb::table(name = people)]
pub struct Person {
    name: String
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

    def test_new_user_flow(self):
        """Test the entirety of the new user flow."""

        ## Publish your module
        self.new_identity(email=None)

        self.publish_module()

        # Calling our database
        self.call("say_hello")
        self.assertIn("Hello, World!", self.logs(2))

        ## Calling functions with arguments
        self.call("add", "Tyler")
        self.call("say_hello")
        self.assertEqual(self.logs(5).count("Hello, World!"), 2)
        self.assertEqual(self.logs(5).count("Hello, Tyler!"), 1)

        out = self.spacetime("sql", self.address, "SELECT * FROM people")
        # The spaces after the name are important
        self.assertMultiLineEqual(out, """\
 name    
---------
 "Tyler" 
""")
