"""A machine that takes Kerberos tickets, for the tests: the Kerberos library's own
side of showing a ticket, behind a port.

A client sends its token, the length in four bytes before it, and gets the answer the
same way, until both are content. From then on the client sends sealed messages, and
gets each back sealed, with the account's name before it: the name the library read in
the ticket.

It is started by real.sh, inside the container of the tests' Kerberos server, and
holds the key of the service the tests ask tickets for.
"""

import socket
import struct
import sys

import gssapi

PORT = 4088


def read(connection):
    """One message, or nothing where the client has gone."""
    arrived = b""
    while len(arrived) < 4:
        part = connection.recv(4 - len(arrived))
        if not part:
            return None
        arrived += part
    (length,) = struct.unpack(">I", arrived)
    message = b""
    while len(message) < length:
        part = connection.recv(length - len(message))
        if not part:
            return None
        message += part
    return message


def write(connection, message):
    connection.sendall(struct.pack(">I", len(message)) + message)


def serve(connection):
    context = gssapi.SecurityContext(usage="accept")
    while not context.complete:
        token = read(connection)
        if token is None:
            return
        write(connection, context.step(token) or b"")
    who = str(context.initiator_name).encode()
    while True:
        sealed = read(connection)
        if sealed is None:
            return
        opened = context.unwrap(sealed)
        if not opened.encrypted:
            raise ValueError("the message was not sealed")
        write(connection, context.wrap(who + b" heard: " + opened.message, True).message)


def main():
    listening = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listening.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listening.bind(("0.0.0.0", PORT))
    listening.listen(8)
    print("the machine is ready", flush=True)
    while True:
        connection, _ = listening.accept()
        connection.settimeout(10)
        try:
            serve(connection)
        except Exception as error:  # a refusal ends the connection, and is said in the log
            print(f"the machine refused: {error}", file=sys.stderr, flush=True)
        finally:
            connection.close()


main()
