import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import ts from "typescript";

const root = process.cwd();

async function importTranspiledTs(path) {
  const source = readFileSync(join(root, path), "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText;
  const tempDir = mkdtempSync(join(tmpdir(), "tokenscope-test-"));
  const tempFile = join(tempDir, "module.mjs");
  writeFileSync(tempFile, output, "utf8");

  try {
    return await import(pathToFileURL(tempFile).href);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

test("app update state recovers stale transient updater state after relaunch", async () => {
  const { normalizeAppUpdateInfo, recoverStoredAppUpdateInfo } = await importTranspiledTs(
    "src/services/appUpdateState.ts",
  );

  for (const status of ["checking", "downloading", "installing"]) {
    const recovered = recoverStoredAppUpdateInfo(
      normalizeAppUpdateInfo({
        available: true,
        current_version: "0.1.12",
        version: "0.1.13",
        date: "2026-06-08T09:00:00.000Z",
        body: "release notes",
        status,
        checked_at: "2026-06-08T09:00:00.000Z",
        error: null,
      }),
    );

    assert.equal(recovered.status, "idle");
    assert.equal(recovered.available, false);
    assert.equal(recovered.current_version, null);
    assert.equal(recovered.version, null);
    assert.equal(recovered.date, null);
    assert.equal(recovered.body, null);
    assert.equal(recovered.checked_at, null);
    assert.equal(recovered.error, null);
  }

  const available = recoverStoredAppUpdateInfo(
    normalizeAppUpdateInfo({
      available: true,
      version: "0.1.13",
      status: "available",
    }),
  );

  assert.equal(available.status, "available");
  assert.equal(available.available, true);
  assert.equal(available.version, "0.1.13");
});
