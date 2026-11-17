# A VNC server on Linux for Away's tests: one password, and a desktop that shows one
# picture and nothing that moves. The pipeline starts it on every change.
FROM debian:13-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2

RUN apt-get update \
 && apt-get install --yes --no-install-recommends tigervnc-standalone-server tigervnc-tools imagemagick \
 && rm -rf /var/lib/apt/lists/*

# the picture the desktop shows: every colour in it, and no two rows alike
RUN convert -size 1280x800 -seed 7 plasma:fractal -depth 8 /usr/local/share/desktop.png

COPY start.sh /usr/local/bin/start
RUN chmod 755 /usr/local/bin/start

EXPOSE 5900
ENTRYPOINT ["/usr/local/bin/start"]
