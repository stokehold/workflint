"""Usage: serve.py <root> <port file> <latest tag>

Serves the files under <root> over HTTP on 127.0.0.1 for test-install.sh.
Writes the port to <port file> once the server listens.

Answers like github.com for the paths that the install scripts read:

- A path that ends in /releases/latest redirects with 302 to the sibling
  /releases/tag/<latest tag>.
- A path under /releases/tag/ returns 200 with an empty body.

Exits by itself after 30 minutes, in case nothing stops it.
"""

import functools
import http.server
import os
import sys
import threading

LIFETIME_SECONDS = 30 * 60


class Handler(http.server.SimpleHTTPRequestHandler):
    latest_tag = ""

    def _github(self):
        path = self.path.split("?", 1)[0]
        if path.endswith("/releases/latest"):
            host, port = self.server.server_address[:2]
            prefix = path[: -len("/latest")]
            self.send_response(302)
            self.send_header("Location", f"http://{host}:{port}{prefix}/tag/{self.latest_tag}")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return True
        if "/releases/tag/" in path:
            self.send_response(200)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return True
        return False

    def do_GET(self):
        if not self._github():
            super().do_GET()

    def do_HEAD(self):
        if not self._github():
            super().do_HEAD()


def main():
    if len(sys.argv) != 4:
        print("usage: serve.py <root> <port file> <latest tag>", file=sys.stderr)
        sys.exit(2)
    root, port_file, latest_tag = sys.argv[1:]
    Handler.latest_tag = latest_tag
    handler = functools.partial(Handler, directory=root)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Timer(LIFETIME_SECONDS, lambda: os._exit(0)).start()
    with open(port_file + ".tmp", "w", encoding="ascii") as f:
        f.write(str(server.server_address[1]))
    os.replace(port_file + ".tmp", port_file)
    server.serve_forever()


if __name__ == "__main__":
    main()
