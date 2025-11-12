import json
import toml
import unittest

from .. import Smoketest, parse_sql_result, random_string

class CreateChildDatabase(Smoketest):
    AUTOPUBLISH = False

    def test_create_child_database(self):
        """
        Test that the owner can add a child database,
        and that deleting the parent also deletes the child.
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

class PermissionsTest(Smoketest):
    AUTOPUBLISH = False

    def create_identity(self):
        """
        Obtain a fresh identity and token from the server.
        Doesn't alter the config.toml for this test instance.
        """
        resp = self.api_call("POST", "/v1/identity")
        return json.loads(resp)

    def create_collaborators(self, database):
        """
        Create collaborators for the current database, one for each role.
        """
        collaborators = {}
        roles = ["Owner", "Admin", "Developer", "Viewer"]
        for role in roles:
            identity_and_token = self.create_identity()
            self.call_controldb_reducer(
                "upsert_collaborator",
                {"Name": database},
                [f"0x{identity_and_token['identity']}"],
                {role: {}}
            )
            collaborators[role] = identity_and_token
        return collaborators


    def call_controldb_reducer(self, reducer, *args):
        """
        Call a controldb reducer.
        """
        self.spacetime("call", "spacetime-control", reducer, *map(json.dumps, args))

    def login_with(self, identity_and_token: dict):
        self.spacetime("logout")
        config = toml.load(self.config_path)
        config['spacetimedb_token'] = identity_and_token['token']
        with open(self.config_path, 'w') as f:
            toml.dump(config, f)

    def publish_as(self, role_and_token, module, code, clear = False):
        print(f"publishing {module} as {role_and_token[0]}:")
        print(f"{code}")
        self.login_with(role_and_token[1])
        self.write_module_code(code)
        self.publish_module(module, clear = clear)
        return self.database_identity

    def sql_as(self, role_and_token, database, sql):
        """
        Log in as `token` and run an SQL statement against `database`
        """
        print(f"running sql as {role_and_token[0]}: {sql}")
        self.login_with(role_and_token[1])
        res = self.spacetime("sql", database, sql)
        return parse_sql_result(str(res))

    def subscribe_as(self, role_and_token, *queries, n):
        """
        Log in as `token` and subscribe to the current database using `queries`.
        """
        print(f"subscribe as {role_and_token[0]}: {queries}")
        self.login_with(role_and_token[1])
        return self.subscribe(*queries, n = n)


@unittest.skip("sql permissions not yet supported")
class MutableSql(PermissionsTest):
    MODULE_CODE = """
#[spacetimedb::table(name = person, public)]
struct Person {
    name: String,
}
"""
    def test_permissions_for_mutable_sql_transactions(self):
        """
        Tests that only owners and admins can perform mutable SQL transactions.
        """

        name = random_string()
        self.publish_module(name)
        team = self.create_collaborators(name)

        for role, token in team.items():
            self.login_with(token)
            dml = f"insert into person (name) values ('bob-the-{role}')"
            if role == "Owner" or role == "Admin":
                self.spacetime("sql", name, dml)
            else:
                with self.assertRaises(Exception):
                    self.spacetime("sql", name, dml)


class PublishDatabase(PermissionsTest):
    MODULE_CODE = """
#[spacetimedb::table(name = person, public)]
struct Person {
    name: String,
}
"""

    MODULE_CODE_OWNER = MODULE_CODE + """
#[spacetimedb::table(name = owner)]
struct Owner {
    name: String,
}
"""

    MODULE_CODE_ADMIN = MODULE_CODE_OWNER + """
#[spacetimedb::table(name = admin)]
struct Admin {
    name: String,
}
"""

    MODULE_CODE_DEVELOPER = MODULE_CODE_ADMIN + """
#[spacetimedb::table(name = developer)]
struct Developer {
    name: String,
}
"""

    MODULE_CODE_VIEWER = MODULE_CODE_DEVELOPER + """
#[spacetimedb::table(name = viewer)]
struct Viewer {
    name: String,
}
"""

    def test_permissions_publish(self):
        """
        Tests that only owner, admin and developer roles can publish a database.
        """

        parent = random_string()
        self.publish_module(parent)

        (owner, admin, developer, viewer) = self.create_collaborators(parent).items()
        succeed_with = [
            (owner, self.MODULE_CODE_OWNER),
            (admin, self.MODULE_CODE_ADMIN),
            (developer, self.MODULE_CODE_DEVELOPER)
        ]

        for role_and_token, code in succeed_with:
            self.publish_as(role_and_token, parent, code)

        with self.assertRaises(Exception):
            self.publish_as(viewer, parent, self.MODULE_CODE_VIEWER)

        # Create a child database.
        child = random_string()
        child_path = f"{parent}/{child}"

        # Developer and viewer should not be able to create a child.
        for role_and_token in [developer, viewer]:
            with self.assertRaises(Exception):
                self.publish_as(role_and_token, child_path, self.MODULE_CODE)
        # But admin should succeed.
        self.publish_as(admin, child_path, self.MODULE_CODE)

        # Once created, only viewer should be denied updating.
        for role_and_token, code in succeed_with:
            self.publish_as(role_and_token, child_path, code)

        with self.assertRaises(Exception):
            self.publish_as(viewer, child_path, self.MODULE_CODE_VIEWER)


class ClearDatabase(PermissionsTest):
    def test_permissions_clear(self):
        """
        Tests that only owners and admins can clear a database.
        """

        parent = random_string()
        self.publish_module(parent)
        # First degree owner can clear.
        self.publish_module(parent, clear = True)

        (owner, admin, developer, viewer) = self.create_collaborators(parent).items()

        # Owner and admin collaborators can clear.
        for role_and_token in [owner, admin]:
            self.publish_as(role_and_token, parent, self.MODULE_CODE, clear = True)

        # Others can't.
        for role_and_token in [developer, viewer]:
            with self.assertRaises(Exception):
                self.publish_as(role_and_token, parent, self.MODULE_CODE, clear = True)

        # Same applies to child.
        child = random_string()
        child_path = f"{parent}/{child}"

        self.publish_as(owner, child_path, self.MODULE_CODE)

        for role_and_token in [owner, admin]:
            self.publish_as(role_and_token, parent, self.MODULE_CODE, clear = True)

        for role_and_token in [developer, viewer]:
            with self.assertRaises(Exception):
                self.publish_as(role_and_token, parent, self.MODULE_CODE, clear = True)


class DeleteDatabase(PermissionsTest):
    def delete_as(self, role_and_token, database):
        print(f"delete {database} as {role_and_token[0]}")
        self.login_with(role_and_token[1])
        self.spacetime("delete", "--yes", database)

    def test_permissions_delete(self):
        """
        Tests that only owners can delete databases.
        """

        parent = random_string()
        self.publish_module(parent)
        self.spacetime("delete", "--yes", parent)

        self.publish_module(parent)

        (owner, admin, developer, viewer) = self.create_collaborators(parent).items()

        for role_and_token in [admin, developer, viewer]:
            with self.assertRaises(Exception):
                self.delete_as(role_and_token, parent)

        child = random_string()
        child_path = f"{parent}/{child}"

        # If admin creates a child, they should also be able to delete it,
        # because they are the owner of the child.
        print("publish and delete as admin")
        self.publish_as(admin, child_path, self.MODULE_CODE)
        self.delete_as(admin, child)

        # The owner role should be able to delete.
        print("publish as admin, delete as owner")
        self.publish_as(admin, child_path, self.MODULE_CODE)
        self.delete_as(owner, child)

        # Anyone else should be denied if not direct owner.
        print("publish as owner, deny deletion by admin, developer, viewer")
        self.publish_as(owner, child_path, self.MODULE_CODE)
        for role_and_token in [admin, developer, viewer]:
            with self.assertRaises(Exception):
                self.delete_as(role_and_token, child)

        print("delete child as owner")
        self.delete_as(owner, child)

        print("delete parent as owner")
        self.delete_as(owner, parent)


@unittest.skip("sql permissions not yet supported")
class PrivateTables(PermissionsTest):
    def test_permissions_private_tables(self):
        """
        Test that all collaborators can read private tables.
        """

        parent = random_string()
        self.publish_module(parent)

        team = self.create_collaborators(parent)
        owner = ("Owner", team['Owner'])

        self.sql_as(owner, parent, "insert into person (name) values ('horsti')")

        for role_and_token in team.items():
            rows = self.sql_as(role_and_token, parent, "select * from person")
            self.assertEqual(rows, [{ "name": '"horsti"' }])

        for role_and_token in team.items():
            sub = self.subscribe_as(role_and_token, "select * from person", n = 2)
            self.sql_as(owner, parent, "insert into person (name) values ('hansmans')")
            self.sql_as(owner, parent, "delete from person where name = 'hansmans'")
            res = sub()
            self.assertEqual(
                res,
                [
                    {
                        'person': {
                            'deletes': [],
                            'inserts': [{'name': 'hansmans'}]
                        }
                    },
                    {
                        'person': {
                            'deletes': [{'name': 'hansmans'}],
                            'inserts': []
                        }
                    }
                ],
            )
