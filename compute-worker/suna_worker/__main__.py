from .worker import serve


if __name__ == "__main__":
    import sys

    serve(sys.stdin.buffer, sys.stdout.buffer)
