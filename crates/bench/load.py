# Mini-tool for executing load testing and call reducer functions.
import argparse
import subprocess
import sys
import time
from datetime import datetime, timedelta


class ProgressBar:
    def __init__(self, total: int, label: str, size: int):
        """
        Initialize the progress bar.

        Args:
            total (int): The total number of steps/items.
            label (str): Label for the progress bar.
            size (int): The width of the progress bar.
        """
        self.total = total
        self.label = label
        self.size = size
        self.current = 0
        self.suffix = ""

    def show(self):
        progress = int(self.size * self.current / self.total)
        bar = "█" * progress + "." * (self.size - progress)
        print(f"{self.label} {bar} {self.current}/{self.total} {self.suffix}", end="\r", flush=True)

    def step(self, steps: int = 1):
        self.current = min(self.current + steps, self.total)
        self.show()

    def finish(self):
        self.current = self.total
        self.show()
        print()


def _run(progress: ProgressBar, title: str, cli: str, database: str, cmd: list):
    for reducer in cmd:
        progress.label = title
        progress.suffix = f' {reducer}'
        progress.show()
        subprocess.check_call(f'{cli} call {database} {reducer}', shell=True)
        progress.step()


def run(cli: str, database: str, init: list, load: list, frequency: float, duration: float):
    print(f'Running load testing for database: {database}')
    print(f"    Frequency: {frequency} calls/second, Duration: {duration} seconds")
    print()
    if init:
        progress = ProgressBar(len(init), label="Processing...", size=20)
        _run(progress, f'Init reducers for database: {database}', cli, database, init)
        progress.finish()

    if load:
        start_time = datetime.now()
        end_time = start_time + timedelta(seconds=duration)
        current_time = start_time
        interval = 1.0 / frequency

        progress = ProgressBar(int(duration * frequency) * len(load), label="Processing...", size=20)
        while current_time < end_time:
            _run(progress, f'Load reducers for database: {database}', cli, database, load)
            time.sleep(interval)
            current_time = datetime.now()
        progress.finish()

    print(f'Load testing for database: {database} finished.')


if __name__ == '__main__':
    """
    Usage:
        python load.py -d <database> -i <init_reducers> -l <load_reducers> [--no-cli] [-f <frequency>] [-s <seconds>]
        
    Example:
        python load.py -d quickstart -f 2 -s 10 -i "insert_bulk_small_rows 100" -l "queries 'small, inserts:10,query:10,deletes:10';"  
    """
    parser = argparse.ArgumentParser()

    parser.add_argument('-d', '--database', type=str, help='Database name', required=True)
    parser.add_argument('-i', '--init', type=str, help='Init reducers, separated by ;')
    parser.add_argument('-l', '--load', type=str, help='Load reducers, separated by ;')
    parser.add_argument('-f', '--frequency', type=float, default=1.0,
                        help="Frequency (calls per second)")
    parser.add_argument('-s', '--seconds', type=float, default=1.0, help="Duration (in seconds)")
    parser.add_argument('--no-cli', action='store_false', dest='cli',
                        help='Disable spacetime-cli if true, run `cargo run...` instead',
                        default=True)

    args = vars(parser.parse_args())

    database = args['database']
    cli = args['cli']
    frequency = args['frequency']
    duration = args['seconds']

    init = [x.strip() for x in (args['init'] or '').split(';') if x.strip()]
    load = [x.strip() for x in (args['load'] or '').split(';') if x.strip()]

    if cli:
        cli = '/Users/mamcx/.cargo/bin/spacetimedb-cli'
    else:
        cli = 'cargo run -p spacetimedb-cli --bin spacetimedb-cli'

    run(cli, database, init, load, frequency, duration)
