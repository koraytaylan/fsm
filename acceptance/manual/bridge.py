"""Expose an existing disposable-job MCP socket to a genuine stdio client."""
import socket
import sys
import threading


def main():
    address = sys.argv[1]
    if not address.startswith('fsm-live-') or len(address) != 41:
        raise ValueError('invalid owned live-session socket')
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(930)
        connection.connect('\0' + address)

        def incoming():
            try:
                while encoded := sys.stdin.buffer.readline(1_048_577):
                    if len(encoded) > 1_048_576:
                        raise ValueError('client frame exceeds bound')
                    connection.sendall(encoded)
            finally:
                connection.shutdown(socket.SHUT_WR)

        worker = threading.Thread(target=incoming, daemon=True)
        worker.start()
        with connection.makefile('rb') as output:
            while encoded := output.readline(1_048_577):
                if len(encoded) > 1_048_576:
                    raise ValueError('server frame exceeds bound')
                sys.stdout.buffer.write(encoded)
                sys.stdout.buffer.flush()


if __name__ == '__main__':
    main()
