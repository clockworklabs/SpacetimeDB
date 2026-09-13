"""Owned-resource failure paths, without contacting Docker or any server."""
import json
import unittest

from acceptance import Builder


class CleanupTests(unittest.TestCase):
    def builder(self, reply):
        builder = Builder.__new__(Builder)
        builder.name = "fixture-owned-name"
        builder.container = None  # docker run reply was lost
        builder.run_attempted = True
        builder.proxy = None
        builder.volume_created = False
        calls = []

        def command(*args):
            calls.append(args)
            if args[:2] == ("container", "inspect"):
                if isinstance(reply, Exception):
                    raise reply
                return 0, json.dumps(reply).encode(), b""
            return 0, b"", b""

        builder.command = command
        return builder, calls

    def owned(self):
        return [{"Name": "/fixture-owned-name", "Id": "owned-id", "Config": {
            "Labels": {"spacetimedb.fixture": "fixture-owned-name"}}}]

    def test_lost_run_reply_removes_only_exact_owned_container(self):
        builder, calls = self.builder(self.owned())
        builder.close()
        self.assertEqual(calls, [
            ("container", "inspect", "fixture-owned-name"),
            ("rm", "--force", "--volumes", "owned-id"),
        ])

    def test_wrong_label_never_authorizes_removal(self):
        reply = self.owned()
        reply[0]["Config"]["Labels"]["spacetimedb.fixture"] = "another-fixture"
        builder, calls = self.builder(reply)
        with self.assertRaisesRegex(RuntimeError, "ownership mismatch"):
            builder.close()
        self.assertEqual(len(calls), 1)

    def test_inspect_error_is_incomplete_cleanup(self):
        builder, calls = self.builder(RuntimeError("daemon unavailable"))
        with self.assertRaisesRegex(RuntimeError, "positive builder teardown failed"):
            builder.close()
        self.assertEqual(len(calls), 1)

    def test_returned_identifier_must_match_inspected_object(self):
        builder, calls = self.builder(self.owned())
        builder.container = "different-id"
        with self.assertRaisesRegex(RuntimeError, "ID mismatch"):
            builder.close()
        self.assertEqual(len(calls), 1)


if __name__ == "__main__":
    unittest.main()
