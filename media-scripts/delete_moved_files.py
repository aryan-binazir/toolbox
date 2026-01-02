#!/usr/bin/env -S uv run
# /// script
# dependencies = ["typer>=0.9.0"]
# ///
"""
Delete files that have been moved to a different directory, usually after a copy function has failed.
"""

from pathlib import Path
import typer

app = typer.Typer(
    help="Delete files that have been moved to a different directory, "
    "usually after a copy function has failed.",
    add_completion=False,
)


@app.command()
def main(
    from_dir: Path = typer.Option(
        Path("."),
        "--from-dir",
        help="Dir copying files from where files already copied will be deleted from",
    ),
    to_dir: Path = typer.Option(
        Path("."),
        "--to-dir",
        help="Dir copying files from where files already copied will be deleted from",
    ),
    dry_run: bool = typer.Option(
        False,
        "--dry-run",
        "-n",
        help="Preview changes without deleting any directories",
    ),
) -> None:
    w = "world"
    print(f"Hello {w}")
    # guard clauses for issues with dirs
    # iterate through file names store in dict (from_dir)
    # iterate through file names in to_dict, if already in hash run md5 comparison
    # # if exact same file then delete file from to_dict if not dry run
    # # log deleted or would be deleted


if __name__ == "__main__":
    app()
