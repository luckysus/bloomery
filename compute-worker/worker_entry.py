"""PyInstaller entry point for the packaged Bloomery compute worker."""

import sys

from bloomery_worker.worker import serve


def main() -> None:
    serve(sys.stdin.buffer, sys.stdout.buffer)


if __name__ == "__main__":
    main()
