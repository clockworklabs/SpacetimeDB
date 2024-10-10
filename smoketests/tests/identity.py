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

