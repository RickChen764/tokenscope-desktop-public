import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import ts from "typescript";

const root = process.cwd();

async function importFormatModule() {
  const source = readFileSync(join(root, "src/utils/format.ts"), "utf8");
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`;

  return import(moduleUrl);
}

test("compact token formatting keeps overview values scannable", async () => {
  const { formatCompactToken } = await importFormatModule();

  assert.equal(formatCompactToken(999, "zh-CN"), "999");
  assert.equal(formatCompactToken(92_525, "zh-CN"), "92.5K");
  assert.equal(formatCompactToken(352_934, "zh-CN"), "353K");
  assert.equal(formatCompactToken(5_565_817, "zh-CN"), "5.57M");
  assert.equal(formatCompactToken(58_658_412, "zh-CN"), "58.7M");
  assert.equal(formatCompactToken(256_329_325, "zh-CN"), "256M");
  assert.equal(formatCompactToken(1_789_035_957, "zh-CN"), "1.79B");
});
