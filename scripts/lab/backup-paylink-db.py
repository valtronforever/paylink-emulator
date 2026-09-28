"""Consistent local SQLite backup; never upload this file with public fixtures."""
import argparse
import hashlib
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source")
parser.add_argument("destination")
args = parser.parse_args()
source, destination = Path(args.source).resolve(), Path(args.destination).resolve()
if not source.is_file() or destination.exists():
    raise SystemExit("Source must exist and destination must be new")
destination.parent.mkdir(parents=True, exist_ok=True)
with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as src:
    with sqlite3.connect(destination) as dst:
        src.backup(dst)
        if dst.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise SystemExit("Backup failed integrity_check")
print(json.dumps({"captured_utc": datetime.now(timezone.utc).isoformat(),
                  "method": "sqlite_backup_api", "source": str(source),
                  "file": str(destination),
                  "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()}))
