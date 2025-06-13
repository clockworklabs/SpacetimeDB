from .. import Smoketest, random_string
import unittest
import json
import io

TIMESTAMP_TAG = "__timestamp_micros_since_unix_epoch__"

class TimestampRoute(Smoketest):
    AUTO_PUBLISH = False

    def test_timestamp_route(self):
        name = random_string()

        # A request for the timestamp at a non-existent database is an error...
        with self.assertRaises(Exception) as err:
            self.api_call(
                "GET",
                f"/v1/database/{name}/unstable/timestamp",
            )
        # ... with code 404.
        self.assertEqual(err.exception.args[0].status, 404)

        self.publish_module(name)

        # A request for the timestamp at an extant database is a success...
        resp = self.api_call(
            "GET",
            f"/v1/database/{name}/unstable/timestamp",
        )

        # ... and the response body is a SATS-JSON encoded `Timestamp`.
        timestamp = json.load(io.BytesIO(resp))
        self.assertIsInstance(timestamp, dict)
        self.assertIn(TIMESTAMP_TAG, timestamp)
        self.assertIsInstance(timestamp[TIMESTAMP_TAG], int)
