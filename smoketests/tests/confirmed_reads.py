from .. import Smoketest, parse_sql_result

#
# TODO: We only test that we can pass a --confirmed flag and that things
# appear to works as if we hadn't. Without controlling the server, we can't
# test that there is any difference in behavior.
#

class ConfirmedReads(Smoketest):
    def test_confirmed_reads_receive_updates(self):
        """Tests that subscribing with confirmed=true receives updates"""

        sub = self.subscribe("select * from person", n = 2, confirmed = True)
        self.call("add", "Horst")
        self.spacetime(
            "sql",
            self.database_identity,
            "insert into person (name) values ('Egon')")

        events = sub()
        self.assertEqual([
            {
                'person': {
                    'deletes': [],
                    'inserts': [{'name': 'Horst'}]
                }
            },
            {
                'person': {
                    'deletes': [],
                    'inserts': [{'name': 'Egon'}]
                }
            }
        ], events)

class ConfirmedReadsSql(Smoketest):
    def test_sql_with_confirmed_reads_receives_result(self):
        """Tests that an SQL operations with confirmed=true returns a result"""

        self.spacetime(
            "sql",
            "--confirmed",
            self.database_identity,
            "insert into person (name) values ('Horst')")

        res = self.spacetime(
            "sql",
            "--confirmed",
            self.database_identity,
            "select * from person")
        res = parse_sql_result(str(res))
        self.assertEqual([{'name': '"Horst"'}], res)
