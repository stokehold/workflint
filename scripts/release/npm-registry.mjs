// Usage: node npm-registry.mjs <packages> <port file>
//
// Serves the npm tarballs in <packages> on 127.0.0.1 as an npm registry, so
// that smoke-test-npm.sh installs them the way a user would: npm, pnpm, and
// Bun fetch workflint's metadata, resolve its optional dependencies, and
// download only the platform package that matches the machine. Writes the
// port into <port file> once it listens, and serves until it is killed.
//
// For each tarball, GET /<name> answers with the package's metadata, and
// GET /-/<file> with the tarball. Every other request gets a 404, so an
// install that needs any other package fails.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";

const [packagesDir, portFile] = process.argv.slice(2);
if (!packagesDir || !portFile) {
  console.error("usage: node npm-registry.mjs <packages> <port file>");
  process.exit(2);
}

// The bytes of <name> in the uncompressed tar archive <tar>.
function tarEntry(tar, name) {
  for (let offset = 0; offset + 512 <= tar.length; ) {
    const header = tar.subarray(offset, offset + 512);
    const field = (start, length) => header.subarray(start, start + length).toString("utf8").replace(/\0.*$/s, "");
    const entry = field(0, 100);
    if (entry === "") break;
    const prefix = field(345, 155);
    const size = Number.parseInt(field(124, 12).trim() || "0", 8);
    const path = prefix ? `${prefix}/${entry}` : entry;
    if (path === name) return tar.subarray(offset + 512, offset + 512 + size);
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  throw new Error(`no ${name} in the tarball`);
}

const tarballs = new Map();
const packuments = new Map();
for (const file of readdirSync(packagesDir).filter((name) => name.endsWith(".tgz")).sort()) {
  const bytes = readFileSync(join(packagesDir, file));
  const manifest = JSON.parse(tarEntry(gunzipSync(bytes), "package/package.json").toString("utf8"));
  tarballs.set(file, bytes);
  packuments.set(manifest.name, { file, manifest, bytes });
}

const server = createServer((request, response) => {
  const url = decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname);
  console.log(`${request.method} ${url}`);
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.writeHead(405).end();
    return;
  }
  if (url.startsWith("/-/") && tarballs.has(url.slice(3))) {
    const bytes = tarballs.get(url.slice(3));
    response.writeHead(200, { "content-type": "application/octet-stream", "content-length": bytes.length });
    response.end(bytes);
    return;
  }
  const found = packuments.get(url.slice(1));
  if (found === undefined) {
    response.writeHead(404, { "content-type": "application/json" }).end('{"error":"not found"}');
    return;
  }
  const { file, manifest, bytes } = found;
  const { address, port } = server.address();
  const version = {
    ...manifest,
    _id: `${manifest.name}@${manifest.version}`,
    dist: {
      tarball: `http://${address}:${port}/-/${file}`,
      shasum: createHash("sha1").update(bytes).digest("hex"),
      integrity: `sha512-${createHash("sha512").update(bytes).digest("base64")}`,
    },
  };
  // A fixed date long ago, so that no package manager holds the version
  // back for being new.
  const time = "2000-01-01T00:00:00.000Z";
  const body = JSON.stringify({
    _id: manifest.name,
    name: manifest.name,
    "dist-tags": { latest: manifest.version },
    versions: { [manifest.version]: version },
    time: { created: time, modified: time, [manifest.version]: time },
  });
  response.writeHead(200, { "content-type": "application/json" });
  response.end(body);
});

server.listen(0, "127.0.0.1", () => {
  // Written whole and then renamed, so that a reader never sees half of it.
  writeFileSync(`${portFile}.tmp`, `${server.address().port}\n`);
  renameSync(`${portFile}.tmp`, portFile);
  console.log(`serving ${[...packuments.keys()].join(", ")} on 127.0.0.1:${server.address().port}`);
});
