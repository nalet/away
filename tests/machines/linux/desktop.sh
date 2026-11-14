#!/bin/sh
# The desktop the tests look at: one picture on the screen, and nothing that moves. It
# stays for as long as the session does.
display -window root /usr/local/share/desktop.png
exec sleep infinity
