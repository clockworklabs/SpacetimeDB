from .. import Smoketest, extract_field
import re

class Servers(Smoketest):
    AUTOPUBLISH = False

    def test_servers(self):
        """Verify that we can add and list server configurations"""

        out = self.spacetime("server", "add", "--url", "https://testnet.spacetimedb.com", "testnet", "--no-fingerprint")
        self.assertEqual(extract_field(out, "Host:"), "testnet.spacetimedb.com")
        self.assertEqual(extract_field(out, "Protocol:"), "https")

        servers = self.spacetime("server", "list")
        self.assertRegex(servers, re.compile(r"^\s*testnet\.spacetimedb\.com\s+https\s+testnet\s*$", re.M))
        self.assertRegex(servers, re.compile(r"^\s*\*\*\*\s+127\.0\.0\.1:3000\s+http\s+localhost\s*$", re.M))

        out = self.spacetime("server", "fingerprint", "http://127.0.0.1:3000", "-y")
        self.assertIn("No saved fingerprint for server 127.0.0.1:3000.", out)

        out = self.spacetime("server", "fingerprint", "http://127.0.0.1:3000")
        self.assertIn("Fingerprint is unchanged for server 127.0.0.1:3000", out)

        out = self.spacetime("server", "fingerprint", "localhost")
        self.assertIn("Fingerprint is unchanged for server localhost", out)

    def test_edit_server(self):
        """Verify that we can edit server configurations"""

        out = self.spacetime("server", "add", "--url", "https://foo.com", "foo", "--no-fingerprint")
        out = self.spacetime("server", "edit", "foo", "--url", "https://testnet.spacetimedb.com", "--new-name", "testnet", "--no-fingerprint", "--yes")
        self.assertEqual(extract_field(out, "Host:"), "testnet.spacetimedb.com")
        self.assertEqual(extract_field(out, "Protocol:"), "https")

        servers = self.spacetime("server", "list")
        self.assertRegex(servers, re.compile(r"^\s*testnet\.spacetimedb\.com\s+https\s+testnet\s*$", re.M))
        self.assertRegex(servers, re.compile(r"^\s*\*\*\*\s+127\.0\.0\.1:3000\s+http\s+localhost\s*$", re.M))

        self.spacetime("server", "remove", "foo", "--yes")
