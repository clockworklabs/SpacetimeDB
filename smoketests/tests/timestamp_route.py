from .. import Smoketest, random_string
import unittest
import json
import io

TIMESTAMP_TAG = "__timestamp_micros_since_unix_epoch__"

class TimestampRoute(Smoketest):
    AUTOPUBLISH = False

    def test_timestamp_route(self):
        resp = self.api_call(
            "GET",
            "/unstable/timestamp",
        )
        timestamp = json.load(io.BytesIO(resp))
        self.assertIsInstance(timestamp, dict)
        self.assertIn(TIMESTAMP_TAG, timestamp)
        self.assertIsInstance(timestamp[TIMESTAMP_TAG], int)
