#!/usr/bin/env python3

import tempfile
import unittest
from pathlib import Path

from discord_summary import GREEN, RED, YELLOW, build_payload, field_value, load_reports


def report(
    *,
    language: str = "csharp",
    mode: str = "guidelines",
    model: str = "test-model",
    tasks: str = "36/37 (97.3%)",
    action: str | None = None,
    pattern: str | None = None,
    classification: str = "Model limitation",
) -> str:
    recommended = action or "No repository changes recommended."
    failures = (
        f"### {pattern} (1 task)\n\n- **Classification:** {classification}\n"
        if pattern
        else "No failures detected.\n"
    )
    return f"""# LLM Benchmark Analysis

- Language: {language}
- Mode: {mode}
- Model: {model}
- Tasks: {tasks}
- Scorers: 100/100 (100.0%)

## Recommended actions

{recommended}

## Failure patterns

{failures}"""


class DiscordSummaryTests(unittest.TestCase):
    def load(self, *contents: str):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, content in enumerate(contents):
                (root / f"report-{index}.md").write_text(content, encoding="utf-8")
            return load_reports(root)

    def test_aggregates_rates_and_formats_findings(self):
        reports = self.load(
            report(tasks="36/37 (97.3%)"),
            report(
                language="rust",
                tasks="34/37 (91.9%)",
                pattern="Incorrect sum-type syntax",
            ),
        )

        embed = build_payload(reports, "https://example.com/run", "Weekly run")["embeds"][0]

        self.assertEqual(embed["description"], "**70/74 (94.6%)** task runs passed")
        self.assertEqual(embed["color"], YELLOW)
        self.assertIn("**C#:** 36/37 (97.3%)", embed["fields"][0]["value"])
        self.assertIn("Rust / guidelines / test-model", embed["fields"][2]["value"])

    def test_action_items_force_review_status(self):
        reports = self.load(
            report(
                action="- **[Skill problem | High] Clarify transactions** — Update the skill. Evidence: t_075.",
            )
        )

        embed = build_payload(reports, "https://example.com/run", "Weekly run")["embeds"][0]

        self.assertEqual(embed["color"], YELLOW)
        self.assertIn("C# / guidelines", embed["fields"][1]["value"])

    def test_healthy_and_infrastructure_colors(self):
        healthy = build_payload(
            self.load(report(tasks="37/37 (100.0%)")),
            "https://example.com/run",
            "Weekly run",
        )["embeds"][0]
        infrastructure = build_payload(
            self.load(
                report(
                    tasks="37/37 (100.0%)",
                    pattern="Provider timeout",
                    classification="Infrastructure/provider problem",
                )
            ),
            "https://example.com/run",
            "Weekly run",
        )["embeds"][0]

        self.assertEqual(healthy["color"], GREEN)
        self.assertEqual(infrastructure["color"], RED)

    def test_empty_and_long_fields_stay_valid(self):
        payload = build_payload([], "https://example.com/run", "Manual run")
        empty = payload["embeds"][0]
        long_value = field_value([f"- {index} {'x' * 400}" for index in range(6)], "None", "finding")

        self.assertEqual(payload["allowed_mentions"], {"parse": []})
        self.assertEqual(empty["color"], RED)
        self.assertIn("No analysis reports", empty["fields"][0]["value"])
        self.assertLessEqual(len(long_value), 1000)


if __name__ == "__main__":
    unittest.main()
