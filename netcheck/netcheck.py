#!/usr/bin/env -S uv run
# /// script
# dependencies = ["typer>=0.9.0"]
# ///
"""
Internet connection monitor.

Pings periodically, logs status, plays sound on drops.
"""

import subprocess
import time
from datetime import datetime
from pathlib import Path

import typer

app = typer.Typer(add_completion=False)

GREEN = "\033[0;32m"
RED = "\033[0;31m"
NC = "\033[0m"

SOUND_FILE = Path("/usr/share/sounds/freedesktop/stereo/bell.oga")


def get_timestamp() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def ping(host: str, timeout: int = 2) -> bool:
    result = subprocess.run(
        ["ping", "-c", "1", "-W", str(timeout), host],
        capture_output=True,
    )
    return result.returncode == 0


def check_connection() -> bool:
    return ping("8.8.8.8") or ping("1.1.1.1")


def play_alert() -> None:
    if SOUND_FILE.exists():
        subprocess.Popen(
            ["paplay", str(SOUND_FILE)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    else:
        print("\a", end="", flush=True)


def log(file: Path, message: str) -> None:
    with file.open("a") as f:
        f.write(f"{get_timestamp()} | {message}\n")


@app.command()
def main(
    interval: float = typer.Option(
        2.0,
        "--interval",
        "-i",
        help="Ping interval in seconds",
    ),
    log_dir: Path = typer.Option(
        None,
        "--log-dir",
        "-d",
        help="Directory for log files (default: script directory)",
    ),
) -> None:
    """Monitor internet connection and log drops."""
    if log_dir is None:
        log_dir = Path(__file__).parent

    connection_log = log_dir / "connection.log"
    drops_log = log_dir / "drops.log"

    print(f"Network connection monitor started (interval: {interval}s)")
    print(f"Logs: {connection_log}, {drops_log}")
    print("Press Ctrl+C to stop\n")

    prev_state = True
    down_since: float | None = None

    try:
        while True:
            current_state = check_connection()
            ts = get_timestamp()

            if not current_state and prev_state:
                # Connection just dropped
                down_since = time.time()
                log(connection_log, "DOWN")
                log(drops_log, "DOWN")
                play_alert()
                print(f"{ts} | {RED}DOWN{NC}")

            elif current_state and not prev_state:
                # Connection restored
                duration = int(time.time() - down_since) if down_since else 0
                msg = f"UP (restored after {duration}s)"
                log(connection_log, msg)
                print(f"{ts} | {GREEN}UP{NC} (restored after {duration}s)")
                down_since = None

            elif not current_state:
                print(f"{ts} | {RED}DOWN{NC}")

            else:
                print(f"{ts} | {GREEN}UP{NC}")

            prev_state = current_state
            time.sleep(interval)

    except KeyboardInterrupt:
        print("\nStopping network monitor...")


if __name__ == "__main__":
    app()
