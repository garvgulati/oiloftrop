const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  RuntimePackageRegistry,
  getRuntimeAssets,
  getRuntimeTextFiles,
  shouldPublishRuntimeFileChange,
} = require("./dev-server");

test("publishes direct inputs but waits for Vite to finish source and dist changes", () => {
  const addonPath = path.join(os.tmpdir(), "example-addon");
  const config = {
    addonPath,
    buildCommand: "pnpm run build",
    manifestPath: path.join(addonPath, "manifest.json"),
  };

  assert.equal(
    shouldPublishRuntimeFileChange(config, path.join(addonPath, "src", "index.ts")),
    false,
  );
  assert.equal(
    shouldPublishRuntimeFileChange(config, path.join(addonPath, "dist", "addon.js")),
    false,
  );
  assert.equal(
    shouldPublishRuntimeFileChange(config, path.join(addonPath, "assets", "logo.png")),
    true,
  );
  assert.equal(shouldPublishRuntimeFileChange(config, config.manifestPath), true);
});

test("indexes static assets while preserving generated JavaScript and CSS", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-assets-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist", "assets"), { recursive: true });
  fs.writeFileSync(path.join(addonPath, "assets", "logo.png"), Buffer.from([1, 2, 3]));
  fs.writeFileSync(path.join(addonPath, "assets", ".gitkeep"), "");
  fs.writeFileSync(path.join(addonPath, "dist", "addon.js"), "export default () => {};");
  fs.writeFileSync(path.join(addonPath, "dist", "assets", "lazy.js"), "export const x = 1;");
  fs.writeFileSync(path.join(addonPath, "dist", "assets", "addon.css"), ".card {}");

  const assets = getRuntimeAssets(addonPath);
  assert.equal(assets.length, 1);
  assert.deepEqual(assets[0].descriptor, {
    id: createHash("sha256")
      .update("assets/logo.png")
      .update("\0")
      .update(Buffer.from([1, 2, 3]))
      .digest("hex"),
    mimeType: "image/png",
    path: "assets/logo.png",
    size: 3,
  });

  const runtimeFiles = getRuntimeTextFiles(addonPath);
  assert.deepEqual(
    runtimeFiles.map((file) => [file.name, file.isMain]),
    [
      ["dist/addon.js", true],
      ["dist/assets/addon.css", false],
      ["dist/assets/lazy.js", false],
    ],
  );
});

test("applies the runtime file-count limit across code and assets", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-entry-limit-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist"), { recursive: true });
  for (let index = 0; index < 128; index += 1) {
    fs.writeFileSync(path.join(addonPath, "dist", `chunk-${index}.js`), "");
  }
  for (let index = 0; index < 129; index += 1) {
    fs.writeFileSync(path.join(addonPath, "assets", `asset-${index}.png`), "");
  }

  assert.throws(() => getRuntimeAssets(addonPath), /more than 256 files/);
  assert.throws(() => getRuntimeTextFiles(addonPath), /more than 256 files/);
});

test("applies the runtime size limit across code and assets", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-size-limit-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist"), { recursive: true });
  const fiveMiB = 5 * 1024 * 1024;
  for (let index = 0; index < 3; index += 1) {
    const filePath = path.join(addonPath, "dist", `chunk-${index}.js`);
    fs.writeFileSync(filePath, "");
    fs.truncateSync(filePath, fiveMiB);
  }
  for (let index = 0; index < 2; index += 1) {
    const filePath = path.join(addonPath, "assets", `asset-${index}.png`);
    fs.writeFileSync(filePath, "");
    fs.truncateSync(filePath, fiveMiB);
  }
  fs.writeFileSync(path.join(addonPath, "assets", "overflow.png"), "x");

  assert.throws(() => getRuntimeAssets(addonPath), /exceeds the 25 MiB package limit/);
  assert.throws(() => getRuntimeTextFiles(addonPath), /exceeds the 25 MiB package limit/);
});

test("reuses one runtime package snapshot until the next generation is published", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-cache-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist"), { recursive: true });
  fs.writeFileSync(path.join(addonPath, "assets", "logo.png"), Buffer.from([1, 2, 3]));
  fs.writeFileSync(path.join(addonPath, "dist", "addon.js"), "export default () => {};\n");

  const registry = new RuntimePackageRegistry(addonPath);
  registry.refresh();
  const firstGeneration = registry.getPackage().generation;
  const firstAssets = registry.getAssets();
  const firstFiles = registry.getTextFiles();
  assert.equal(firstAssets[0].descriptor.size, 3);
  assert.equal(firstFiles[0].content, "export default () => {};\n");

  fs.appendFileSync(path.join(addonPath, "assets", "logo.png"), Buffer.from([4]));
  fs.writeFileSync(path.join(addonPath, "assets", "second.png"), Buffer.from([5]));
  fs.writeFileSync(path.join(addonPath, "dist", "addon.css"), ".changed {}\n");

  assert.equal(registry.getAssets(), firstAssets);
  assert.equal(registry.getTextFiles(), firstFiles);
  assert.equal(registry.getAssets().length, 1);
  assert.equal(registry.getAssets()[0].descriptor.size, 3);
  assert.equal(registry.getTextFiles().length, 1);

  registry.refresh();
  const secondGeneration = registry.getPackage().generation;
  const refreshedAssets = registry.getAssets();
  const refreshedFiles = registry.getTextFiles();
  assert.notEqual(refreshedAssets, firstAssets);
  assert.notEqual(refreshedFiles, firstFiles);
  assert.equal(refreshedAssets.length, 2);
  assert.equal(refreshedAssets[0].descriptor.size, 4);
  assert.equal(refreshedFiles.length, 2);
  assert.equal(secondGeneration, firstGeneration + 1);
  assert.equal(registry.getAssets(firstGeneration), firstAssets);
  assert.deepEqual(registry.getAssets(firstGeneration)[0].content, Buffer.from([1, 2, 3]));
  assert.notEqual(refreshedAssets[0].descriptor.id, firstAssets[0].descriptor.id);
});

test("content-addresses same-size asset replacements", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-generation-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist"), { recursive: true });
  fs.writeFileSync(path.join(addonPath, "dist", "addon.js"), "export default () => {};\n");
  fs.writeFileSync(path.join(addonPath, "assets", "logo.png"), Buffer.from([1, 2, 3]));

  const registry = new RuntimePackageRegistry(addonPath);
  registry.refresh();
  const first = registry.getPackage();
  fs.writeFileSync(path.join(addonPath, "assets", "logo.png"), Buffer.from([4, 5, 6]));
  const second = registry.refresh();

  assert.notEqual(first.assets[0].descriptor.id, second.assets[0].descriptor.id);
  assert.deepEqual(first.assets[0].content, Buffer.from([1, 2, 3]));
  assert.deepEqual(second.assets[0].content, Buffer.from([4, 5, 6]));
});

test("retains the four most recent runtime generations", (context) => {
  const addonPath = fs.mkdtempSync(path.join(os.tmpdir(), "wealthfolio-addon-retention-"));
  context.after(() => fs.rmSync(addonPath, { force: true, recursive: true }));
  fs.mkdirSync(path.join(addonPath, "assets"), { recursive: true });
  fs.mkdirSync(path.join(addonPath, "dist"), { recursive: true });
  fs.writeFileSync(path.join(addonPath, "dist", "addon.js"), "export default () => {};\n");
  fs.writeFileSync(path.join(addonPath, "assets", "value.txt"), "1");

  const registry = new RuntimePackageRegistry(addonPath);
  for (let generation = 1; generation <= 5; generation += 1) {
    fs.writeFileSync(path.join(addonPath, "assets", "value.txt"), String(generation));
    registry.refresh();
  }

  assert.equal(registry.getPackage().generation, 5);
  assert.throws(() => registry.getAssets(1), /no longer available/);
  assert.deepEqual(registry.getAssets(2)[0].content, Buffer.from("2"));
});
