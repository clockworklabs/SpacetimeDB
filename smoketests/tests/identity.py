from .. import Smoketest, extract_field

class IdentityImports(Smoketest):
    AUTOPUBLISH = False

    def setUp(self):
        self.reset_config()
     
    def test_import(self):
        """
        Try to import a known good identity to our local ~/.spacetime/config.toml file
        
        This test does not require a remote spacetimedb instance.
        """

        identity = self.new_identity()
        token = self.token(identity)

        self.reset_config()

        self.fingerprint()

        self.import_identity(identity, token, default=True)
        # [ "$(grep "$IDENT" "$TEST_OUT" | awk '{print $1}')" == '***' ]

    def test_remove(self):
        """Test deleting an identity from your local ~/.spacetime/config.toml file."""
    
        self.fingerprint()

        self.new_identity()
        identity = self.new_identity()
        identities = self.spacetime("identity", "list")
        self.assertIn(identity, identities)

        self.spacetime("identity", "remove", "--identity", identity)
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity, identities)

        with self.assertRaises(Exception):
            self.spacetime("identity", "remove", "--identity", identity)

    def test_remove_all(self):
        """Test deleting all identities with --yes"""

        self.fingerprint()

        identity1 = self.new_identity()
        identity2 = self.new_identity()
        identities = self.spacetime("identity", "list")
        self.assertIn(identity2, identities)

        self.spacetime("identity", "remove", "--identity", identity2)
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity2, identities)

        self.spacetime("identity", "remove", "--all", "--yes")
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity1, identities)

    def test_set_default(self):
        """Ensure that we are able to set a default identity"""

        self.fingerprint()

        self.new_identity()
        identity = self.new_identity()

        identities = self.spacetime("identity", "list").splitlines()
        default_identity = next(filter(lambda s: "***" in s, identities), "")
        self.assertNotIn(identity, default_identity)
        
        self.spacetime("identity", "set-default", "--identity", identity)

        identities = self.spacetime("identity", "list").splitlines()
        default_identity = next(filter(lambda s: "***" in s, identities), "")
        self.assertIn(identity, default_identity)

    def test_set_email(self):
        """Ensure that we are able to associate an email with an identity"""

        self.fingerprint()

        # Create a new identity
        identity = self.new_identity(email=None)
        email = random_email()
        token = self.token(identity)

        # Reset our config so we lose this identity
        self.reset_config()

        # Import this identity, and set it as the default identity
        self.import_identity(identity, token, default=True)

        # Configure our email
        output = self.spacetime("identity", "set-email", "--identity", identity, email)
        self.assertEqual(extract_field(output, "IDENTITY"), identity)
        self.assertEqual(extract_field(output, "EMAIL").lower(), email.lower())

        # Reset config again
        self.reset_config()

        # Find our identity by its email
        output = self.spacetime("identity", "find", email)
        self.assertEqual(extract_field(output, "IDENTITY"), identity)
        self.assertEqual(extract_field(output, "EMAIL").lower(), email.lower())


class IdentityFormatting(Smoketest):
    MODULE_CODE = """
use log::info;
use spacetimedb::{Address, Identity, ReducerContext, Table};

#[spacetimedb::table(name = connected_client)]
pub struct ConnectedClient {
    identity: Identity,
    address: Address,
}

#[spacetimedb::reducer(client_connected)]
fn on_connect(ctx: &ReducerContext) {
    ctx.db.connected_client().insert(ConnectedClient {
        identity: ctx.sender,
        address: ctx.address.expect("sender address unset"),
    });
}

#[spacetimedb::reducer(client_disconnected)]
fn on_disconnect(ctx: &ReducerContext) {
    let sender_identity = &ctx.sender;
    let sender_address = ctx.address.as_ref().expect("sender address unset");
    let match_client = |row: &ConnectedClient| {
        &row.identity == sender_identity && &row.address == sender_address
    };
    if let Some(client) = ctx.db.connected_client().iter().find(match_client) {
        ctx.db.connected_client().delete(client);
    }
}

#[spacetimedb::reducer]
fn print_num_connected(ctx: &ReducerContext) {
    let n = ctx.db.connected_client().count();
    info!("CONNECTED CLIENTS: {n}")
}
"""

    def test_identity_formatting(self):
        """Tests formatting of Identity."""

        # Start two subscribers
        self.subscribe("SELECT * FROM connected_client", n=2)
        self.subscribe("SELECT * FROM connected_client", n=2)

        # Assert that we have two clients + the reducer call
        self.call("print_num_connected")
        logs = self.logs(10)
        self.assertEqual("CONNECTED CLIENTS: 3", logs.pop())

