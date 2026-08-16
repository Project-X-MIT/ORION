"""Expose backup freshness and drill status as Prometheus metrics."""

from __future__ import annotations

import http.server
import os
import pathlib
import time


BACKUP_DIR = pathlib.Path(os.environ.get("ORION_BACKUP_DIR", "/var/lib/orion/backups"))
MAX_AGE = int(os.environ.get("ORION_BACKUP_MAX_AGE_SECONDS", "86400"))


def metric_text() -> str:
    now = int(time.time())
    backups = sorted(BACKUP_DIR.glob("*.dump.enc"), key=lambda path: path.stat().st_mtime)
    latest = backups[-1] if backups else None
    latest_timestamp = int(latest.stat().st_mtime) if latest else 0
    age = max(0, now - latest_timestamp) if latest else 2**31
    success_marker = BACKUP_DIR / ".restore-test-success"
    failure_marker = BACKUP_DIR / ".last-failure"
    last_failure = int(failure_marker.stat().st_mtime) if failure_marker.exists() else 0
    failure = int(last_failure > latest_timestamp)
    restore_test = int(success_marker.exists() and (not latest or success_marker.stat().st_mtime >= latest.stat().st_mtime))
    return "\n".join(
        [
            "# HELP orion_backup_age_seconds Age of the newest encrypted backup.",
            "# TYPE orion_backup_age_seconds gauge",
            f"orion_backup_age_seconds {age}",
            "# HELP orion_backup_max_age_seconds Configured maximum backup age.",
            "# TYPE orion_backup_max_age_seconds gauge",
            f"orion_backup_max_age_seconds {MAX_AGE}",
            "# HELP orion_backup_last_success_timestamp_seconds Unix timestamp of the newest encrypted backup.",
            "# TYPE orion_backup_last_success_timestamp_seconds gauge",
            f"orion_backup_last_success_timestamp_seconds {latest_timestamp}",
            "# HELP orion_backup_failure Whether a failed backup is newer than the last successful backup.",
            "# TYPE orion_backup_failure gauge",
            f"orion_backup_failure {failure}",
            "# HELP orion_backup_restore_test_success Whether the latest restore drill marker is present.",
            "# TYPE orion_backup_restore_test_success gauge",
            f"orion_backup_restore_test_success {restore_test}",
            "",
        ]
    )


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802
        if self.path not in ("/metrics", "/health"):
            self.send_error(404)
            return
        body = b"ok\n" if self.path == "/health" else metric_text().encode()
        content_type = "text/plain; version=0.0.4" if self.path == "/metrics" else "text/plain"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format: str, *_args: object) -> None:
        return


if __name__ == "__main__":
    http.server.ThreadingHTTPServer(("0.0.0.0", 9101), Handler).serve_forever()
