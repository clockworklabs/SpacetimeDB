export const presets = {
  report: {
    code: `import csv
import io
from pathlib import Path

data = """product,quantity
Notebook,3
Pen,12
Notebook,2
Pencil,8"""
orders = list(csv.DictReader(io.StringIO(data)))
items = sum(int(order["quantity"]) for order in orders)
report = f"{len(orders)} orders\\n{items} items\\n"
Path("/tmp/report.txt").write_text(report)
print(report, end="")
print("Saved /tmp/report.txt")`,
  },
  read: {
    code: `from pathlib import Path

print(Path("/tmp/report.txt").read_text(), end="")`,
  },
};

export function pythonCommand(code: string): string {
  return `python3 -c '${code.replace(/'/g, "'\\''")}'`;
}
