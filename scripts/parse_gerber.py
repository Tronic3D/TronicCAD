#!/usr/bin/env python3
"""Checks that every manufacturing file in a directory parses with gerbonara, a
third-party library sharing no code with TronicCAD. Development tool only."""
import sys
import warnings
from pathlib import Path

from gerbonara import ExcellonFile, GerberFile

directory = Path(sys.argv[1])
count = 0
for path in sorted(directory.iterdir()):
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", SyntaxWarning)
        if path.suffix == ".gbr":
            GerberFile.open(path)
        elif path.suffix == ".drl":
            ExcellonFile.open(path)
        else:
            continue
    count += 1
print(f"All files parsed: {count}")
sys.exit(0 if count else 1)
