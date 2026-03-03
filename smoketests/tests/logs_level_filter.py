import json
from .. import Smoketest


class LogsLevelFilter(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn log_all_levels(ctx: &ReducerContext) {
    log::trace!("msg-trace");
    log::debug!("msg-debug");
    log::info!("msg-info");
    log::warn!("msg-warn");
    log::error!("msg-error");
}
"""

    def _logs_filtered(self, n, *extra_args):
        """Get log records as parsed JSON dicts with extra CLI args."""
        self._check_published()
        output = self.spacetime(
            "logs", "--format=json", "-n", str(n),
            *extra_args, "--", self.database_identity,
        )
        return list(map(json.loads, output.strip().splitlines())) if output.strip() else []

    @staticmethod
    def _messages(records):
        return [r["message"] for r in records]

    def test_logs_no_filter(self):
        """Without --level, all log levels are returned."""
        self.call("log_all_levels")

        messages = self.logs(100)
        self.assertIn("msg-trace", messages)
        self.assertIn("msg-debug", messages)
        self.assertIn("msg-info", messages)
        self.assertIn("msg-warn", messages)
        self.assertIn("msg-error", messages)

    def test_logs_level_minimum(self):
        """--level filters to that level and above."""
        self.call("log_all_levels")

        # --level warn: only warn and error
        records = self._logs_filtered(100, "--level", "warn")
        messages = self._messages(records)
        self.assertNotIn("msg-trace", messages)
        self.assertNotIn("msg-debug", messages)
        self.assertNotIn("msg-info", messages)
        self.assertIn("msg-warn", messages)
        self.assertIn("msg-error", messages)

        # --level error: only error
        records = self._logs_filtered(100, "--level", "error")
        messages = self._messages(records)
        self.assertNotIn("msg-trace", messages)
        self.assertNotIn("msg-debug", messages)
        self.assertNotIn("msg-info", messages)
        self.assertNotIn("msg-warn", messages)
        self.assertIn("msg-error", messages)

    def test_logs_level_exact(self):
        """--level-exact shows only the specified level."""
        self.call("log_all_levels")

        # --level info --level-exact: only info
        records = self._logs_filtered(100, "--level", "info", "--level-exact")
        messages = self._messages(records)
        self.assertNotIn("msg-trace", messages)
        self.assertNotIn("msg-debug", messages)
        self.assertIn("msg-info", messages)
        self.assertNotIn("msg-warn", messages)
        self.assertNotIn("msg-error", messages)

        # --level debug --level-exact: only debug
        records = self._logs_filtered(100, "--level", "debug", "--level-exact")
        messages = self._messages(records)
        self.assertNotIn("msg-trace", messages)
        self.assertIn("msg-debug", messages)
        self.assertNotIn("msg-info", messages)
        self.assertNotIn("msg-warn", messages)
        self.assertNotIn("msg-error", messages)
