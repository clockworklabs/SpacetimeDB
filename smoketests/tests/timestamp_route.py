from .. import Smoketest, random_string
import unittest
import json
import io

TIMESTAMP_TAG = "__timestamp_micros_since_unix_epoch__"

class TimestampRoute(Smoketest):
    AUTO_PUBLISH = False

    def test_timestamp_route(self):
        name = random_string()

        with self.assertRaises(Exception) as err:
            self.api_call(
                "GET",
                f"/unstable/database/{name}/timestamp",
            )

        self.assertEqual(err.exception.args[0].status, 404)

        self.publish_module(name)

        resp = self.api_call(
            "GET",
            f"/unstable/database/{name}/timestamp",
        )

        timestamp = json.load(io.BytesIO(resp))
        self.assertIsInstance(timestamp, dict)
        self.assertIn(TIMESTAMP_TAG, timestamp)
        self.assertIsInstance(timestamp[TIMESTAMP_TAG], int)
