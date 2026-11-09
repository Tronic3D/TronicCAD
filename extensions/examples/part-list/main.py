"""An example extension for TronicCAD: a part list as CSV.

Reads the project as JSON from standard input and answers with JSON on standard output.
See docs/EXTENSIONS.md.
"""
import csv
import io
import json
import sys


def main():
    project = json.load(sys.stdin)
    parts = [
        (component.get("reference", ""), component.get("value", ""), component.get("symbol", ""))
        for sheet in project["schematic"].get("sheets", [])
        for component in sheet.get("components", [])
    ]
    parts.sort()
    table = io.StringIO()
    writer = csv.writer(table, lineterminator="\n")
    writer.writerow(["Reference", "Value", "Symbol"])
    writer.writerows(parts)
    json.dump(
        {
            "messages": [{"level": "info", "text": f"{len(parts)} components listed"}],
            "files": {"part-list.csv": table.getvalue()},
        },
        sys.stdout,
    )


if __name__ == "__main__":
    main()
