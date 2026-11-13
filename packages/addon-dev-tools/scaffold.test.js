const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { AddonScaffold } = require("./scaffold");

test("pins the supported browser floor in generated addon projects", async (context) => {
  const targetDir = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-scaffold-"));
  context.after(() => fs.rmSync(targetDir, { force: true, recursive: true }));

  await new AddonScaffold().createAddon({ name: "Browser Target" }, targetDir);

  const viteConfig = fs.readFileSync(path.join(targetDir, "vite.config.ts"), "utf8");
  assert.match(viteConfig, /target: \["chrome107", "edge107", "firefox104", "safari16"\]/);

  const packageJson = JSON.parse(fs.readFileSync(path.join(targetDir, "package.json"), "utf8"));
  assert.match(packageJson.scripts.package, /mkdir -p dist assets/);
});
