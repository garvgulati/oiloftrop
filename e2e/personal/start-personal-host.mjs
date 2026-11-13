import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, openSync, closeSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import http from "node:http";
import https from "node:https";

// This extends the documented disposable-profile production-build workflow.
// No .env is rewritten and no installed runtime, owner key or Tailscale is used.
const root = mkdtempSync(join(tmpdir(), "oiloftrop-personal-e2e-"));
const state = join(root, "state");
const env = {};
for (const key of [
  "SystemRoot",
  "WINDIR",
  "PATH",
  "PATHEXT",
  "COMSPEC",
  "USERPROFILE",
  "APPDATA",
  "LOCALAPPDATA",
  "TEMP",
  "TMP",
]) {
  if (process.env[key]) env[key] = process.env[key];
}
Object.assign(env, { WF_RUNTIME_PURPOSE: "personal", WF_RUNTIME_BOUNDARY_VERSION: "1" });
const server =
  process.env.WF_PERSONAL_E2E_SERVER ||
  resolve("target/debug", process.platform === "win32" ? "wealthfolio-server.exe" : "wealthfolio-server");
const init = spawnSync(server, ["personal", "init", state, "--password-stdin"], {
  cwd: root,
  env,
  input: "Synthetic personal browser password 42",
  encoding: "utf8",
  windowsHide: true,
});
assert.equal(init.status, 0, "Disposable personal provisioning failed");
JSON.parse(init.stdout); // Validate, but do not print even synthetic recovery material.
const certificate = spawnSync(
  "pwsh",
  ["-NoLogo", "-NoProfile", "-File", resolve("e2e/personal/test-certificate.ps1")],
  {
    cwd: process.cwd(),
    env,
    encoding: "utf8",
    windowsHide: true,
  },
);
assert.equal(certificate.status, 0, "Loopback test certificate could not be prepared");
Object.assign(env, {
  WF_LISTEN_ADDR: "127.0.0.1:18488",
  WF_DB_PATH: join(state, "data/app.db"),
  WF_DATA_DIR: join(state, "data"),
  WF_SECRET_KEY_FILE: join(state, "master-key.txt"),
  WF_SECRET_FILE: join(state, "data/secrets.json"),
  WF_DB_REQUIRE_ENCRYPTION: "true",
  WF_AUTH_PASSWORD_HASH: readFileSync(join(state, "password-hash.txt"), "utf8").trim(),
  WF_COOKIE_SECURE: "true",
  WF_CORS_ALLOW_ORIGINS: "https://127.0.0.1:18489",
  WF_MCP_ENABLED: "false",
  WF_STATIC_DIR: resolve("dist"),
  RUST_LOG: "warn",
});
const log = openSync(join(root, "synthetic-server.log"), "a");
const child = spawn(server, [], { cwd: root, env, stdio: ["ignore", log, log], windowsHide: true });
closeSync(log);
let stopping = false;
function stop() {
  if (stopping) return;
  stopping = true;
  child.kill();
  proxy.close(() => process.exit(0));
  setTimeout(() => process.exit(1), 2000).unref();
}
child.on("error", () => {
  console.error("Disposable personal server could not start");
  stop();
});
child.on("exit", () => {
  if (!stopping) {
    console.error("Disposable personal server exited");
    stop();
  }
});
const proxy = https.createServer(
  {
    pfx: Buffer.from(certificate.stdout.trim(), "base64"),
    passphrase: "synthetic-test-certificate",
  },
  (request, response) => {
    const upstream = http.request(
      {
        host: "127.0.0.1",
        port: 18488,
        path: request.url,
        method: request.method,
        headers: request.headers,
      },
      (incoming) => {
        response.writeHead(incoming.statusCode || 502, incoming.headers);
        incoming.pipe(response);
      },
    );
    upstream.on("error", () => {
      response.writeHead(503);
      response.end("Test server starting");
    });
    request.pipe(upstream);
  },
);
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
proxy.listen(18489, "127.0.0.1", () =>
  console.log("Disposable encrypted personal test proxy ready"),
);
