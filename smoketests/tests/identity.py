from .. import Smoketest, random_string, extract_field

def random_email():
    return random_string() + "@clockworklabs.io"

class IdentityImports(Smoketest):
    AUTOPUBLISH = False

    def setUp(self):
        self.reset_config()
     
    def test_import(self):
        """
        Try to import a known good identity to our local ~/.spacetime/config.toml file
        
        This test does not require a remote spacetimedb instance.
        """

        identity = self.new_identity(email=None)
        token = self.token(identity)

        self.reset_config()

        self.fingerprint()

        self.import_identity(identity, token, default=True)
        # [ "$(grep "$IDENT" "$TEST_OUT" | awk '{print $1}')" == '***' ]

    def test_new_email(self):
        """This test is designed to make sure an email can be set while creating a new identity"""

        # Create a new identity
        email = random_email()
        identity = self.new_identity(email=email)
        token = self.token(identity)

        # Reset our config so we lose this identity
        self.reset_config()

        # Import this identity, and set it as the default identity
        self.import_identity(identity, token, default=True)

        # Configure our email
        output = self.spacetime("identity", "set-email", "--identity", identity, email)
        self.assertEqual(extract_field(output, "IDENTITY"), identity)
        self.assertEqual(extract_field(output, "EMAIL"), email)

        # Reset config again
        self.reset_config()

        # Find our identity by its email
        output = self.spacetime("identity", "find", email)
        self.assertEqual(extract_field(output, "IDENTITY"), identity)
        self.assertEqual(extract_field(output, "EMAIL").lower(), email.lower())
    
    def test_remove(self):
        """Test deleting an identity from your local ~/.spacetime/config.toml file."""
    
        self.fingerprint()

        self.new_identity(email=None)
        identity = self.new_identity(email=None)
        identities = self.spacetime("identity", "list")
        self.assertIn(identity, identities)

        self.spacetime("identity", "remove", "--identity", identity)
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity, identities)

        with self.assertRaises(Exception):
            self.spacetime("identity", "remove", "--identity", identity)

    def test_remove_all(self):
        """Test deleting all identities with --force"""

        self.fingerprint()

        identity1 = self.new_identity(email=None)
        identity2 = self.new_identity(email=None)
        identities = self.spacetime("identity", "list")
        self.assertIn(identity2, identities)

        self.spacetime("identity", "remove", "--identity", identity2)
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity2, identities)

        self.spacetime("identity", "remove", "--all", "--force")
        identities = self.spacetime("identity", "list")
        self.assertNotIn(identity1, identities)

    def test_set_default(self):
        """Ensure that we are able to set a default identity"""

        self.fingerprint()

        self.new_identity(email=None)
        identity = self.new_identity(email=None)

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

