from .. import Smoketest, random_string

class Domains(Smoketest):
    AUTOPUBLISH = False

    def test_register_domain(self):
        """Attempts to register some valid domains and makes sure invalid domains cannot be registered"""

        rand_domain = random_string()

        self.new_identity()
        self.spacetime("dns", "register-tld", rand_domain)

        self.publish_module(rand_domain)
        self.publish_module(f"{rand_domain}/test")
        self.publish_module(f"{rand_domain}/test/test2")

        with self.assertRaises(Exception):
            self.publish_module(f"{rand_domain}//test")
        with self.assertRaises(Exception):
            self.publish_module(f"{rand_domain}/test/")
        with self.assertRaises(Exception):
            self.publish_module(f"{rand_domain}/test//test2")
    
    def test_reverse_dns(self):
        """This tests the functionality of spacetime reverse dns lookups"""

        rand_domain = random_string()
        self.spacetime("dns", "register-tld", rand_domain)

        self.publish_module(rand_domain)

        names = self.spacetime("dns", "reverse-lookup", self.resolved_identity).splitlines()
        self.assertIn(rand_domain, names)

    def test_set_name(self):
        """Tests the functionality of the set-name command"""

        self.publish_module()

        rand_name = random_string()

        self.spacetime("dns", "register-tld", rand_name)
        self.spacetime("dns", "set-name", rand_name, self.database_identity)
        lookup_result = self.spacetime("dns", "lookup", rand_name).strip()
        self.assertEqual(lookup_result, self.database_identity)

        names = self.spacetime("dns", "reverse-lookup", self.database_identity).splitlines()
        self.assertIn(rand_name, names)
