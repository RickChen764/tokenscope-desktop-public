import { execFileSync } from "node:child_process";
import { join } from "node:path";

const prettierExtensions = new Set([
  ".css",
  ".json",
  ".md",
  ".mjs",
  ".ts",
  ".tsx",
]);
const prettierFiles = changedFiles().filter((file) =>
  prettierExtensions.has(extensionOf(file)),
);

if (prettierFiles.length === 0) {
  console.log("No changed files require Prettier checks.");
  process.exit(0);
}

const prettier = join(
  process.cwd(),
  "node_modules",
  "prettier",
  "bin",
  "prettier.cjs",
);
execFileSync(process.execPath, [prettier, "--check", ...prettierFiles], {
  stdio: "inherit",
});

function changedFiles() {
  const tracked = execFileSync("git", [
    "-c",
    "core.autocrlf=false",
    "diff",
    "--name-only",
    "--diff-filter=ACMRT",
    "HEAD",
    "--",
  ]).toString();
  const untracked = execFileSync("git", [
    "-c",
    "core.autocrlf=false",
    "ls-files",
    "--others",
    "--exclude-standard",
  ]).toString();
  return `${tracked}\n${untracked}`
    .split(/\r?\n/)
    .map((file) => file.trim())
    .filter(Boolean)
    .filter((file, index, files) => files.indexOf(file) === index);
}

function extensionOf(file) {
  const dotIndex = file.lastIndexOf(".");
  return dotIndex === -1 ? "" : file.slice(dotIndex);
}
