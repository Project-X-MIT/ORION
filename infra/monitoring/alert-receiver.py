#!/usr/bin/env python3
"""Synthetic staging alert sink; never use this as a production receiver."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802 - stdlib handler API
        if self.path == "/health":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok\n")
            return
        self.send_error(404)

    def do_POST(self):  # noqa: N802 - stdlib handler API
        if self.path != "/alerts":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length", "0"))
        payload = json.loads(self.rfile.read(length))
        alerts = payload.get("alerts", [])
        summary = [
            {
                "status": alert.get("status"),
                "name": alert.get("labels", {}).get("alertname"),
                "owner": alert.get("labels", {}).get("owner"),
                "runbook": alert.get("annotations", {}).get("runbook"),
            }
            for alert in alerts
        ]
        print(json.dumps(summary, sort_keys=True), flush=True)
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"accepted\n")

    def log_message(self, _format, *_args):
        return


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8081"))
    ThreadingHTTPServer(("0.0.0.0", port), Handler).serve_forever()
