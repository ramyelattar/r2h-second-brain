from pathlib import Path
import pgserver

data_dir = Path(r"E:\Projects\r2h-second-brain\run-data\khoj\postgres")
server = pgserver.get_server(str(data_dir), cleanup_mode="stop")

try:
    for zone in ("UTC", "GMT", "Etc/UTC", "Etc/GMT", "UCT"):
        escaped = zone.replace("'", "''")
        try:
            result = server.psql(
                f"SET TIME ZONE '{escaped}'; "
                "SELECT current_setting('TimeZone');"
            )
            print(f"PASS {zone}: {result.strip()}")
        except Exception as exc:
            print(f"FAIL {zone}: {exc}")
finally:
    server.cleanup()
