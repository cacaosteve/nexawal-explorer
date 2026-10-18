"""Local outage/rate-limit regression for the Mini's custom Caddy image.

Run on the Mini: python3 Scripts/test-incus-proxy.py
Only synthetic requests reach a disposable loopback-bound Caddy container.
"""
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import time


def docker(*args, **kwargs):
    return subprocess.check_output(["docker", *args], **kwargs)


template = (Path(__file__).resolve().parent.parent / "deploy/Caddyfile.incus.example").read_text()
config = template.replace("{\n", "{\n    admin off\n    persist_config off\n", 1)
config = config.replace("explorer.nexawal.com {", "http://explorer.test:8080 {")
config = config.replace("http://10.62.67.10:8088", "http://127.0.0.1:1")
config = config.replace("events 10", "events 2").replace("window 1s", "window 1m")
config += "\nhttp://unrelated.test:8080 {\n reverse_proxy 127.0.0.1:1\n}\n"
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    port = listener.getsockname()[1]
name = f"nexawal-incus-proxy-test-{os.getpid()}"
process = subprocess.Popen([
    "docker", "run", "--rm", "-i", "--name", name,
    "-p", f"127.0.0.1:{port}:8080", "nexatrode-caddy:ratelimit",
    "caddy", "run", "--config", "/dev/stdin", "--adapter", "caddyfile",
], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
process.stdin.write(config.encode())
process.stdin.close()
process.stdin = None


def get(host, path):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
    try:
        headers = {"Host": host}
        if host == "explorer.test":
            headers.update({"X-Forwarded-For": "192.0.2.123", "X-Test": "PRIVATE_HEADER_CANARY"})
        connection.request("GET", path, headers=headers)
        response = connection.getresponse()
        return response.status, dict(response.getheaders()), response.read().decode()
    finally:
        connection.close()


try:
    first = None
    for _ in range(100):
        try:
            first = get("explorer.test", "/tx/" + "a" * 64 + "?probe=PRIVATE_QUERY_CANARY")
            break
        except (OSError, http.client.HTTPException):
            if process.poll() is not None:
                raise RuntimeError(process.stdout.read().decode())
            time.sleep(0.1)
    assert first is not None, "Caddy did not start"
    assert first[0] == 503, first
    assert first[1]["Referrer-Policy"] == "no-referrer", first
    assert first[1]["Cache-Control"] == "no-store", first
    assert get("explorer.test", "/PRIVATE_PATH_CANARY")[0] == 503
    limited = get("explorer.test", "/PRIVATE_RATE_CANARY")
    assert limited[0] == 429, limited
    assert int(limited[1]["Retry-After"]) > 0, limited
    assert get("unrelated.test", "/UNRELATED_LOG_CANARY")[0] == 502
finally:
    subprocess.run(["docker", "stop", "-t", "1", name], check=False,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    output, _ = process.communicate(timeout=10)

logs = output.decode()
for secret in ["a" * 64, "PRIVATE_QUERY_CANARY", "PRIVATE_HEADER_CANARY",
               "PRIVATE_PATH_CANARY", "PRIVATE_RATE_CANARY", "192.0.2.123"]:
    assert secret not in logs, f"Explorer request leaked: {secret}"
assert "UNRELATED_LOG_CANARY" in logs, "Unrelated site's error log lost"
events = []
for line in logs.splitlines():
    try:
        event = json.loads(line)
    except ValueError:
        continue
    if event.get("logger") == "http.handlers.rate_limit":
        events.append(event)
        assert "remote_ip" not in event and "key" not in event, event
assert any(e.get("msg") == "rate limit exceeded" for e in events), events
print("PASS: explorer outage privacy, 429 + Retry-After, redacted rate-limit diagnostics, unrelated errors retained")
