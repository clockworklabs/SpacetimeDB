import string

from .. import Smoketest

class CallReducerProcedure(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ProcedureContext, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
}

#[spacetimedb::procedure]
pub fn return_person(_ctx: &mut ProcedureContext) -> Person {
   return Person { name: "World".to_owned() };
}
"""

    def test_call_reducer_procedure(self):
        """Check calling a reducer (not return) and procedure (return)"""
        msg = self.call("say_hello")
        self.assertEqual(msg, "")
        
        msg = self.call("return_person")
        self.assertEqual(msg.strip(), '["World"]')
    
    def test_call_errors(self):
        """Check calling a non-existent reducer/procedure raises error"""
        out = self.call("non_existent_reducer", check= False, full_output=True).stderr
        identity = self.database_identity
        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent_reducer` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_hello

Here are some existing procedures:
- return_person""".strip())
        
        out = self.call("non_existent_procedure", check= False, full_output=True).stderr
        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent_procedure` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_hello

Here are some existing procedures:
- return_person""".strip())
        
        out = self.call("say_hell", check= False, full_output=True).stderr

        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `say_hell` for database `{identity}` resolving to identity `{identity}`.

A reducer with a similar name exists: `say_hello`""".strip())
        
        out = self.call("return_perso", check= False, full_output=True).stderr
        
        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `return_perso` for database `{identity}` resolving to identity `{identity}`.

A procedure with a similar name exists: `return_person`""".strip())

class CallEmptyReducerProcedure(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}
"""
    def test_call_empty_errors(self):
        """Check calling into a database with no reducers/procedures raises error"""
        out = self.call("non_existent", check= False, full_output=True).stderr
        identity = self.database_identity
        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent` for database `{identity}` resolving to identity `{identity}`.

The database has no reducers.

The database has no procedures.""".strip())

module_template = string.Template("""
#[spacetimedb::reducer]
pub fn say_reducer_$NUM(_ctx: &ReducerContext) {
    log::info!("Hello from reducer $NUM!");
}
#[spacetimedb::procedure]
pub fn say_procedure_$NUM(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure $NUM!");
}
""")

class CallManyReducerProcedure(Smoketest):
    MODULE_CODE = f"""
use spacetimedb::{{log, ProcedureContext, ReducerContext}};
{"".join(module_template.substitute(NUM=i) for i in range(11))}
"""

    def test_call_many_errors(self):
        """Check calling into a database with many reducers/procedures raises error with listing"""
        out = self.call("non_existent", check= False, full_output=True).stderr
        identity = self.database_identity
        self.assertIn(out.strip(), f"""
WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_reducer_0
- say_reducer_1
- say_reducer_2
- say_reducer_3
- say_reducer_4
- say_reducer_5
- say_reducer_6
- say_reducer_7
- say_reducer_8
- say_reducer_9
... (1 reducer not shown)

Here are some existing procedures:
- say_procedure_0
- say_procedure_1
- say_procedure_2
- say_procedure_3
- say_procedure_4
- say_procedure_5
- say_procedure_6
- say_procedure_7
- say_procedure_8
- say_procedure_9
... (1 procedure not shown)
""".strip())