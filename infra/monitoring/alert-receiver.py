#!/usr/bin/env python3
"""Synthetic staging alert sink; never use this as a production receiver."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from email.message import EmailMessage
import json
import os
import smtplib


def send_email(summary):
    """Forward an alert summary when staging SMTP settings are configured."""
    smarthost = os.environ.get("ORION_SMTP_SMARTHOST", "").strip()
    sender = os.environ.get("ORION_SMTP_FROM", "").strip()
    recipient = os.environ.get("ORION_ALERT_EMAIL_TO", "").strip()
    if not smarthost or not sender or not recipient:
        return False

    host, separator, port_text = smarthost.rpartition(":")
    if not separator:
        host, port_text = smarthost, "25"
    message = EmailMessage()
    message["From"] = sender
    message["To"] = recipient
    names = ", ".join(item["name"] or "unknown" for item in summary)
    message["Subject"] = f"[ORION staging] {names}"
    message.set_content(json.dumps(summary, sort_keys=True, indent=2))

    with smtplib.SMTP(host, int(port_text), timeout=10) as smtp:
        smtp.ehlo()
        if os.environ.get("ORION_SMTP_REQUIRE_TLS", "true").lower() == "true":
            smtp.starttls()
            smtp.ehlo()
        username = os.environ.get("ORION_SMTP_USERNAME", "").strip()
        password = os.environ.get("ORION_SMTP_PASSWORD", "")
        if username:
            smtp.login(username, password)
        smtp.send_message(message)
    return True


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
        try:
            if send_email(summary):
                print(json.dumps({"email": "sent"}), flush=True)
        except (OSError, ValueError, smtplib.SMTPException) as error:
            print(json.dumps({"email": "failed", "error": str(error)}), flush=True)
            self.send_error(502, "email delivery failed")
            return
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"accepted\n")

    def log_message(self, _format, *_args):
        return


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8081"))
    ThreadingHTTPServer(("0.0.0.0", port), Handler).serve_forever()
