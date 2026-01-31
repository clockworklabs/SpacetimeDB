import json
import toml

from .. import COMPOSE_FILE, Smoketest, parse_sql_result, random_string, spacetime
from ..docker import DockerManager
from ..tests.replication import Cluster

OWNER = "Owner"
ADMIN = "Admin"
DEVELOPER = "Developer"
VIEWER = "Viewer"

ROLES = [OWNER, ADMIN, DEVELOPER, VIEWER]

def get(d: dict, k):
    return (k, d[k])

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


class ChangeDatabaseHierarchy(Smoketest):
    AUTOPUBLISH = False

    def test_change_database_hierarchy(self):
        """
        Test that changing the hierarchy of an existing database is not
        supported.
        """

        parent_name = f"parent-{random_string()}"
        sibling_name = f"sibling-{random_string()}"
        child_name = f"child-{random_string()}"

        self.publish_module(parent_name)
        self.publish_module(sibling_name)

        # Publish as a child of 'parent_name'.
        self.publish_module(f"{parent_name}/{child_name}")

        # Publishing again with a different parent is rejected...
        with self.assertRaises(Exception):
            self.publish_module(f"{sibling_name}/{child_name}", clear = False)
        # ..even if `clear = True`
        with self.assertRaises(Exception):
            self.publish_module(f"{sibling_name}/{child_name}", clear = True)

        # Publishing again with the same parent is ok.
        self.publish_module(f"{parent_name}/{child_name}", clear = False)


class TeamsPermissionsTest(Smoketest):
    AUTOPUBLISH = False

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.root_config = cls.project_path / "root_config"
        spacetime("--config-path", cls.root_config, "server", "set-default", "local")

    def setUp(self):
        self.docker = DockerManager(COMPOSE_FILE)
        self.root_token = self.docker.generate_root_token()

        self.cluster = Cluster(self.docker, self)

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
        for role in ROLES:
            identity_and_token = self.create_identity()
            self.call_controldb_reducer(
                "upsert_collaborator",
                {"Name": database},
                [f"0x{identity_and_token['identity']}"],
                {role: {}}
            )
            collaborators[role] = identity_and_token
        return collaborators

    def create_organization(self):
        """
        Create an organization with one member per role.
        """
        members = {}
        organization_identity = self.create_identity()['identity']
        for role in ROLES:
            member = self.create_identity()
            self.call_controldb_reducer(
                "upsert_organization_member",
                [f"0x{organization_identity}"],
                [f"0x{member['identity']}"],
                {role: {}}
            )
            members[role] = member
        organization =  {
            "organization": organization_identity,
            "members": members
        }
        self.organization = organization

        return organization

    def make_admin(self):
        """
        Create an admin account for the currently logged-in identity.
        """
        identity = str(self.spacetime("login", "show")).split()[-1]
        spacetime("--config-path", self.root_config, "login", "--token", self.root_token)
        spacetime("--config-path", self.root_config, "call",
                  "spacetime-control", "create_admin_account", f"0x{identity}")

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

    def publish_as(self, role_and_token, module, code = None, clear = False, org = None):
        print(f"publishing {module} with org {org} as {role_and_token[0]}:")
        code = self.MODULE_CODE if code is None else code
        print(f"{code}")
        self.login_with(role_and_token[1])
        self.write_module_code(code)
        self.publish_module(module, clear = clear, organization = org)
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

    def tearDown(self):
        if "organization" in self.__dict__:
            # Log in as owner
            self.login_with(self.organization['members'][OWNER])
            # Delete database (requires org to still exist)
            super().tearDown()
            # Delete org
            try:
                self.call_controldb_reducer(
                    "delete_organization",
                    [f"0x{self.organization['organization']}"]
                )
            except Exception:
                pass
        else:
            super().tearDown()


class TeamsMutableSql(TeamsPermissionsTest):
    MODULE_CODE = """
#[spacetimedb::table(name = person, public)]
struct Person {
    name: String,
}
"""

    def run_test(self, database, team):
        for role, token in team.items():
            self.login_with(token)
            dml = f"insert into person (name) values ('bob-the-{role}')"
            if role == OWNER or role == ADMIN:
                self.spacetime("sql", database, dml)
            else:
                with self.assertRaises(Exception):
                    self.spacetime("sql", database, dml)

class CollaboratorsMutableSql(TeamsMutableSql):
    def test_permissions_mut_sql_collaborators(self):
        """
        Tests that only owner and admin collaborators can perform mutable SQL
        transactions.
        """

        name = random_string()
        self.publish_module(name)
        team = self.create_collaborators(name)
        self.run_test(name, team)


class OrgMutableSql(TeamsMutableSql):
    def test_org_permissions_mut_sql_org_members(self):
        """
        Tests that only owner and admin organization members can perform mutable
        SQL transactions.
        """

        self.make_admin()
        org = self.create_organization()
        name = random_string()

        self.login_with(org['members'][OWNER])
        self.publish_module(name, organization = f"0x{org['organization']}")

        self.run_test(name, org['members'])


class TeamsPublishDatabase(TeamsPermissionsTest):
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

    MODULES = {
        OWNER: MODULE_CODE_OWNER,
        ADMIN: MODULE_CODE_ADMIN,
        DEVELOPER: MODULE_CODE_DEVELOPER,
        VIEWER: MODULE_CODE_VIEWER
    }

    def run_test(self, parent, child, team, org):
        self.assert_all_except_viewer_can_update(parent, team, org = org)

        # Create a child database.
        child_path = f"{parent}/{child}"

        # Developer and viewer should not be able to create a child.
        for role in [DEVELOPER, VIEWER]:
            with self.assertRaises(Exception):
                self.publish_as(get(team, role), child_path, self.MODULE_CODE, org = org)
        # But admin should succeed.
        self.publish_as(get(team, ADMIN), child_path, self.MODULE_CODE, org = org)

        # Once created, only viewer should be denied updating.
        self.assert_all_except_viewer_can_update(child_path, team, org)

    def assert_all_except_viewer_can_update(self, database, team, org):
        for role in [OWNER, ADMIN, DEVELOPER]:
            self.publish_as(get(team, role), database, self.MODULES[role], org = org)

        with self.assertRaises(Exception):
            self.publish_as(get(team, VIEWER), database, self.MODULES[VIEWER], org = org)


class CollaboratorsPublishDatabase(TeamsPublishDatabase):
    def test_permissions_publish_collaborators(self):
        """
        Tests that only owner, admin and developer collaborators can publish a
        database.
        """

        parent = random_string()
        child = random_string()
        self.publish_module(parent)
        team = self.create_collaborators(parent)

        self.run_test(parent, child, team, org = None)


class OrgPublishDatabase(TeamsPublishDatabase):
    def test_permissions_publish_org_members(self):
        """
        Tests that only owner, admin and developer organization members  can
        publish a database.
        """

        self.make_admin()
        org = self.create_organization()
        parent = random_string()
        child = random_string()

        self.login_with(org['members'][OWNER])
        self.publish_module(parent, organization = f"0x{org['organization']}")

        self.run_test(parent, child, org['members'],
                      org = org['organization'])


class TeamsClearDatabase(TeamsPermissionsTest):
    def assert_can_clear(self, auth, database):
        self.publish_as(auth, database, clear = True)

    def assert_cannot_clear(self, auth, database):
        with self.assertRaises(Exception):
            self.publish_as(auth, database, clear = True)

    def assert_clear_permissions(self, team, database):
        for role in [OWNER, ADMIN]:
            self.assert_can_clear(get(team, role), database)

        for role in [DEVELOPER, VIEWER]:
            self.assert_cannot_clear(get(team, role), database)


class CollaboratorsClearDatabase(TeamsClearDatabase):
    def test_permissions_clear_collaborators(self):
        """
        Tests that only owner and admin collaborators can clear a database.
        """

        parent = random_string()
        self.publish_module(parent)
        # First degree owner can clear.
        self.publish_module(parent, clear = True)

        team = self.create_collaborators(parent)
        self.assert_clear_permissions(team, parent)

        # Child databases cannot be cleared at all
        child = f"{parent}/{random_string()}"
        self.publish_as(get(team, OWNER), child)
        for auth in team.items():
            self.assert_cannot_clear(auth, child)


class OrgClearDatabase(TeamsClearDatabase):
    def test_permissions_clear_org(self):
        """
        Test that only owner or admin org members can clear a database.
        """

        self.make_admin()
        org = self.create_organization()
        team = org['members']

        parent = random_string()

        self.login_with(org['members'][OWNER])
        self.publish_module(parent, organization = f"0x{org['organization']}")
        self.assert_clear_permissions(team, parent)

        # Child databases cannot be cleared at all
        child = f"{parent}/{random_string()}"
        self.publish_as(get(team, ADMIN), child)
        for auth in team.items():
            self.assert_cannot_clear(auth, child)


class TeamsDeleteDatabase(TeamsPermissionsTest):
    def delete_as(self, role_and_token, database):
        print(f"delete {database} as {role_and_token[0]}")
        self.login_with(role_and_token[1])
        self.spacetime("delete", "--yes", database)


class CollaboratorsDeleteDatabase(TeamsDeleteDatabase):
    def test_permissions_delete_collaborators(self):
        """
        Tests that only owners can delete databases.
        """

        parent = random_string()
        child = random_string()
        self.publish_module(parent)
        self.spacetime("delete", "--yes", parent)

        self.publish_module(parent)

        team = self.create_collaborators(parent)
        for role in [ADMIN, DEVELOPER, VIEWER]:
            with self.assertRaises(Exception):
                self.delete_as(get(team, role), parent)

        child_path = f"{parent}/{child}"

        # If admin creates a child, they should also be able to delete it,
        # because they are the owner of the child.
        print("publish and delete as admin")
        self.publish_as(get(team, ADMIN), child_path)
        self.delete_as(get(team, ADMIN), child)

        # The owner role should be able to delete.
        print("publish as admin, delete as owner")
        self.publish_as(get(team, ADMIN), child_path)
        self.delete_as(get(team, OWNER), child)

        # Anyone else should be denied if not direct owner.
        print("publish as owner, deny deletion by admin, developer, viewer")
        self.publish_as(get(team, OWNER), child_path)
        for role in [ADMIN, DEVELOPER, VIEWER]:
            with self.assertRaises(Exception):
                self.delete_as(get(team, role), child)

        print("delete child as owner")
        self.delete_as(get(team, OWNER), child)

        print("delete parent as owner")
        self.delete_as(get(team, OWNER), parent)


class OrgDeleteDatabase(TeamsDeleteDatabase):
    def test_permissions_delete_org(self):
        """
        Tests that only organization owners can delete databases.
        """

        self.make_admin()
        org = self.create_organization()
        team = org['members']
        parent = random_string()
        child = random_string()

        self.login_with(org['members'][OWNER])
        self.publish_module(parent, organization = f"0x{org['organization']}")
        self.publish_module(f"{parent}/{child}")

        # Org databases can only be deleted by owners
        # because ownership is transferred to the org
        # and publisher attribution is lost.
        for database in [child, parent]:
            for role in [ADMIN, DEVELOPER, VIEWER]:
                with self.assertRaises(Exception):
                    self.delete_as(get(team, role), database)

        self.delete_as(get(team, OWNER), child)
        self.delete_as(get(team, OWNER), parent)


class TeamsPrivateTables(TeamsPermissionsTest):
    def run_test(self, database, team):
        owner = get(team, OWNER)
        self.sql_as(owner, database, "insert into person (name) values ('horsti')")

        for auth in team.items():
            rows = self.sql_as(auth, database, "select * from person")
            self.assertEqual(rows, [{ "name": '"horsti"' }])

        for auth in team.items():
            sub = self.subscribe_as(auth, "select * from person", n = 2)
            self.sql_as(owner, database, "insert into person (name) values ('hansmans')")
            self.sql_as(owner, database, "delete from person where name = 'hansmans'")
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


class CollaboratorsPrivateTables(TeamsPrivateTables):
    def test_permissions_private_tables(self):
        """
        Test that all collaborators can read private tables.
        """

        database = random_string()
        self.publish_module(database)

        team = self.create_collaborators(database)
        self.run_test(database, team)


class OrgPrivateTables(TeamsPrivateTables):
    def test_org_permissions_private_tables(self):
        """
        Test that all organization members can read private tables.
        """

        self.make_admin()
        org = self.create_organization()
        database = random_string()

        self.login_with(org['members'][OWNER])
        self.publish_module(database, organization = f"0x{org['organization']}")

        self.run_test(database, org['members'])
