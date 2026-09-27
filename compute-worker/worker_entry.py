"""PyInstaller entry point for the packaged Suna compute worker."""

import sys

from suna_worker.worker import serve


def main() -> None:
    serve(sys.stdin.buffer, sys.stdout.buffer)


if __name__ == "__main__":
    main()
