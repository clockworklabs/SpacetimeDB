#!/usr/bin/env python3

import argparse
import json
import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

GREEN = 0x57F287
YELLOW = 0xFEE75C
RED = 0xED4245
MAX_ITEMS = 5
MAX_FIELD_LENGTH = 1000


@dataclass
class Report:
    language: str
    mode: str
    model: str
    passed_tasks: int
    total_tasks: int
    actions: list[str]
    other_findings: list[str]


def field(text: str, name: str) -> str:
    match = re.search(rf"^- {re.escape(name)}: (.+)$", text, re.MULTILINE)
    return match.group(1) if match else "unknown"


def language_name(language: str) -> str:
    return {
        "csharp": "C#",
        "rust": "Rust",
        "typescript": "TypeScript",
    }.get(language, language)


def parse_report(text: str) -> Report:
    language = field(text, "Language")
    mode = field(text, "Mode")
    model = field(text, "Model")
    task_match = re.match(r"(\d+)/(\d+)", field(text, "Tasks"))
    passed_tasks, total_tasks = map(int, task_match.groups()) if task_match else (0, 0)

    actions = []
    recommended = text.partition("## Recommended actions")[2].partition("## Failure patterns")[0]
    for line in recommended.splitlines():
        if line.startswith("- **["):
            actions.append(f"- {language_name(language)} / {mode}: {line[2:]}")

    other_findings = []
    failures = text.partition("## Failure patterns")[2]
    for section in re.split(r"^### ", failures, flags=re.MULTILINE)[1:]:
        heading = section.splitlines()[0]
        title_match = re.match(r"(.+) \(\d+ tasks?\)$", heading)
        title = title_match.group(1) if title_match else heading
        classification = re.search(r"^- \*\*Classification:\*\* (.+)$", section, re.MULTILINE)
        if classification and classification.group(1) in {
            "Model limitation",
            "Infrastructure/provider problem",
            "No action",
        }:
            other_findings.append(
                f"- {language_name(language)} / {mode} / {model}: {title} - {classification.group(1)}"
            )

    return Report(language, mode, model, passed_tasks, total_tasks, actions, other_findings)


def load_reports(reports_dir: Path) -> list[Report]:
    return [
        parse_report(path.read_text(encoding="utf-8"))
        for path in sorted(reports_dir.rglob("*.md"))
    ]


def rate(passed: int, total: int) -> str:
    percent = passed * 100 / total if total else 0
    return f"{passed}/{total} ({percent:.1f}%)"


def field_value(lines: list[str], empty: str, overflow_label: str) -> str:
    unique = list(dict.fromkeys(lines))
    visible = unique[:MAX_ITEMS] or [empty]
    if len(unique) > MAX_ITEMS:
        visible.append(f"- ...and {len(unique) - MAX_ITEMS} more {overflow_label}(s)")
    value = "\n".join(visible)
    if len(value) > MAX_FIELD_LENGTH:
        return value[: MAX_FIELD_LENGTH - 30].rstrip() + "\n... View the full analysis."
    return value


def build_payload(reports: list[Report], run_url: str, run_label: str) -> dict:
    totals = [0, 0]
    by_language: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    actions = []
    other_findings = []

    for report in reports:
        totals[0] += report.passed_tasks
        totals[1] += report.total_tasks
        by_language[report.language][0] += report.passed_tasks
        by_language[report.language][1] += report.total_tasks
        actions.extend(report.actions)
        other_findings.extend(report.other_findings)

    passed, total = totals
    pass_percent = passed * 100 / total if total else 0
    has_infrastructure_failure = any("Infrastructure/provider problem" in item for item in other_findings)
    if not total or pass_percent < 90 or has_infrastructure_failure:
        color = RED
    elif actions or pass_percent < 95:
        color = YELLOW
    else:
        color = GREEN

    language_rates = [
        f"- **{language_name(language)}:** {rate(*counts)}"
        for language, counts in sorted(by_language.items())
    ]
    return {
        "username": "SpacetimeDB LLM Benchmarks",
        "embeds": [
            {
                "title": "LLM Benchmark Analysis",
                "url": run_url,
                "description": f"**{rate(*totals)}** task runs passed",
                "color": color,
                "fields": [
                    {
                        "name": "By language",
                        "value": field_value(
                            language_rates,
                            "No analysis reports were produced.",
                            "language",
                        ),
                        "inline": False,
                    },
                    {
                        "name": "Action items",
                        "value": field_value(actions, "None", "action"),
                        "inline": False,
                    },
                    {
                        "name": "Other failures",
                        "value": field_value(other_findings, "None", "finding"),
                        "inline": False,
                    },
                ],
                "footer": {"text": run_label},
            }
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a Discord embed from LLM benchmark analysis reports.")
    parser.add_argument("--reports-dir", type=Path, required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--run-label", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    payload = build_payload(load_reports(args.reports_dir), args.run_url, args.run_label)
    args.output.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    main()
