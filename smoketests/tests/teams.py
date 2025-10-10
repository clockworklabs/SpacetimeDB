from .. import Smoketest, parse_sql_result, random_string

class CreateChildDatabase(Smoketest):
    AUTOPUBLISH = False

    def test_create_child_database(self):
        """
        Test that the owner can add a child database
        """

        parent_name = random_string()
        child_name = random_string()

        self.publish_module(parent_name)
        parent_identity = self.database_identity
        self.publish_module(f"{parent_name}/{child_name}")
        child_identity = self.database_identity

        databases = self.query_controldb(parent_identity, child_identity)
        self.assertEqual(2, len(databases))

        self.spacetime("delete", "--yes", parent_name)

        databases = self.query_controldb(parent_identity, child_identity)
        self.assertEqual(0, len(databases))

    def query_controldb(self, parent, child):
        res = self.spacetime(
            "sql",
            "spacetime-control",
            f"select * from database where database_identity = 0x{parent} or database_identity = 0x{child}"
        )
        return parse_sql_result(str(res))
