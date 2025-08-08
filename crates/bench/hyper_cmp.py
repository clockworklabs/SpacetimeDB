# Mini-tool for comparing benchmark between PR or locally
import os
import json
import argparse
import subprocess
import shutil


def statDecoder(statDict):
    return namedtuple('Stat', statDict.keys())(*statDict.values())


def clean(stat):
    new = {}
    for (k, v) in stat.items():
        if k in ["stddev", "command", "exit_codes"]:
            continue
        if k == "times":
            v = v[0]
        if isinstance(v, float):
            v = round(v, 2)
        new[k] = v
    return new


def larger(row: list):
    return [len(str(x)) for x in row]


def bar_chart(data: list):
    max_value = max(count for _, count in data.items())
    increment = max_value / 25

    longest_label_length = max(len(label) for label, _ in data.items())

    for label, count in data.items():

        # The ASCII block elements come in chunks of 8, so we work out how
        # many fractions of 8 we need.
        # https://en.wikipedia.org/wiki/Block_Elements
        bar_chunks, remainder = divmod(int(count * 8 / increment), 8)

        # First draw the full width chunks
        bar = '█' * bar_chunks

        # Then add the fractional part.  The Unicode code points for
        # block elements are (8/8), (7/8), (6/8), ... , so we need to
        # work backwards.
        if remainder > 0:
            bar += chr(ord('█') + (8 - remainder))

        # If the bar is empty, add a left one-eighth block
        bar = bar or '▏'

        print(f'{label.rjust(longest_label_length)} ▏ {count:#4f} {bar}')


class Report:
    def __init__(self, title: str, header: list, bar: dict, rows: dict):
        self.title = title
        size = larger(header)
        for row in rows:
            size = size + larger(row)
        self.header = header
        self.bar = bar
        self.rows = rows
        self.larger = max(size) + 2


class Stat:
    def __init__(self, results):
        self.spacetime = clean(results[0])
        self.sqlite = clean(results[1])


def load_file(named: str):
    data = open(named).read()
    return Stat(json.loads(data)['results'])


def print_cell(cell: str, size: int, is_last: bool):
    spaces = " " * (size - len(cell))
    if is_last:
        return "| %s%s |" % (cell, spaces)
    else:
        return "| %s%s " % (cell, spaces)


def print_row(row: list, size: int):
    line = ""
    for (pos, x) in enumerate(row):
        line += print_cell(str(x), size, pos + 1 == len(row))
    print(line)


def print_mkdown(report: Report):
    print("###", report.title)
    print("\n```bash")
    bar_chart(report.bar)
    print("```\n")
    print("*Smaller is better.*")
    print_row(report.header, report.larger)
    print_row(["-" * report.larger for x in report.header], report.larger)

    for row in report.rows:
        print_row(row, report.larger)


def pick_winner(a: dict, b: dict, label_a: str, label_b: str):
    if a["mean"] > b["mean"]:
        winner = label_b
        delta = a["times"]
    else:
        if a["mean"] == b["mean"]:
            winner = "TIE"
            delta = a["times"]
        else:
            winner = label_a
            delta = b["times"]
    return winner, delta


# Check Sqlite VS Spacetime
def cmp_bench(stat: Stat):
    winner, delta = pick_winner(stat.spacetime, stat.sqlite, "Spacetime", "Sqlite")

    header = ["Stat", "Sqlite", "Spacetime", "Delta"]

    rows = []
    for (k, v) in stat.sqlite.items():
        rows.append([k, v, stat.spacetime[k], round(stat.spacetime[k] - v, 2)])

    bar = dict(SpacetimeDB=stat.spacetime["mean"], Sqlite=stat.sqlite["mean"])
    return Report("Comparing Sqlite VS Spacetime Winner: **%s**" % winner, header, bar, rows)


# Check the progress of Spacetime between branches / PR
def improvement_bench(old: Stat, new: Stat):
    winner, delta = pick_winner(old.spacetime, new.spacetime, "Old", "New")

    header = ["Stat", "OLD", "NEW", "Delta"]

    rows = []
    for (k, v) in old.spacetime.items():
        rows.append([k, v, new.spacetime[k], round(v - new.spacetime[k], 2)])

    bar = dict(Old=old.spacetime["mean"], New=new.spacetime["mean"])
    return Report("Improvement of Spacetime. Winner: **%s**" % winner, header, bar, rows)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument("bench", choices=['versus', 'pr'], help="Select bench")

    args = vars(parser.parse_args())

    # args = {"bench": "pr"}
    cmd = "./hyperfine.sh insert"
    if args["bench"] == "pr":
        subprocess.check_call('./pr_copy.sh "%s"' % cmd, shell=True)

        subprocess.check_call(cmd, shell=True, timeout=60 * 5)
        shutil.copyfile("out.json", "new.json")

        old = load_file("old.json")
        new = load_file("new.json")
        report = improvement_bench(old, new)
    else:
        subprocess.check_call(cmd, shell=True, timeout=60 * 5)

        stat = load_file("out.json")
        report = cmp_bench(stat)

    print_mkdown(report)
