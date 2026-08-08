import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = join(frontendRoot, "src");
const mainPath = join(sourceRoot, "main.tsx");
const appPath = join(sourceRoot, "app", "BloomeryApp.tsx");
const bridgePath = join(sourceRoot, "bridge", "desktop.ts");
const failures = [];

for (const [path, label] of [[appPath, "Bloomery app root"], [bridgePath, "desktop bridge"]]) {
  if (!existsSync(path)) failures.push(`missing ${label}: ${relative(frontendRoot, path)}`);
}

const main = readFileSync(mainPath, "utf8");
if (!/from ["']\.\/app\/BloomeryApp["']/.test(main)) {
  failures.push("src/main.tsx must import ./app/BloomeryApp");
}
for (const forbidden of ["AuthProvider", "RagAppPage", "DesktopApp"]) {
  if (main.includes(forbidden)) failures.push(`src/main.tsx contains ${forbidden}`);
}

const productionFiles = [];
const visited = new Set();
const importPattern = /(?:import|export)\s+(?:[^"']*?\sfrom\s*)?["'](\.[^"']+)["']/g;

function resolveModule(from, specifier) {
  const base = resolve(dirname(from), specifier);
  return [base, `${base}.ts`, `${base}.tsx`, join(base, "index.ts"), join(base, "index.tsx")]
    .find((candidate) => existsSync(candidate) && [".ts", ".tsx"].includes(extname(candidate)));
}

function visit(path) {
  if (visited.has(path)) return;
  visited.add(path);
  productionFiles.push(path);
  const text = readFileSync(path, "utf8");
  for (const match of text.matchAll(importPattern)) {
    const dependency = resolveModule(path, match[1]);
    if (dependency) visit(dependency);
  }
}

visit(mainPath);

const forbiddenPatterns = [
  [/["'`]\/api\//, "Web API route"],
  [/cloud_api_base/, "cloud_api_base"],
  [/AuthProvider/, "AuthProvider"],
  [/useAuthSession/, "useAuthSession"],
  [/DesktopCloudTask/, "DesktopCloudTask"],
];

for (const path of productionFiles) {
  const text = readFileSync(path, "utf8");
  const label = relative(frontendRoot, path).replaceAll("\\", "/");
  for (const [pattern, name] of forbiddenPatterns) {
    if (pattern.test(text)) failures.push(`${label} contains ${name}`);
  }
  if (path !== bridgePath && /@tauri-apps\/api\/(core|event)/.test(text)) {
    failures.push(`${label} imports the raw Tauri core/event API`);
  }
}

const sourceWidePatterns = [
  [/["'`]\/api\//, "Web API route"],
  [/cloud_api_base/, "cloud_api_base"],
  [/AuthProvider/, "AuthProvider"],
  [/useAuthSession/, "useAuthSession"],
  [/DesktopCloudTask/, "DesktopCloudTask"],
  [/CloudJob/, "CloudJob"],
  [/LoginPage/, "LoginPage"],
  [/authHeaders/, "authHeaders"],
];

function collectSourceFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collectSourceFiles(path);
    return [".ts", ".tsx"].includes(extname(entry.name)) ? [path] : [];
  });
}

const pageBudget = 300;
for (const path of collectSourceFiles(join(sourceRoot, "features")).filter(
  (candidate) => /Page\.tsx$/.test(candidate) && !/\.test\.tsx$/.test(candidate),
)) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/).length;
  if (lines > pageBudget) {
    failures.push(
      `${relative(frontendRoot, path).replaceAll("\\", "/")} has ${lines} lines; page budget is ${pageBudget}`,
    );
  }
}

for (const path of collectSourceFiles(sourceRoot)) {
  const source = readFileSync(path, "utf8");
  const label = relative(frontendRoot, path).replaceAll("\\", "/");
  for (const [pattern, name] of sourceWidePatterns) {
    if (pattern.test(source)) failures.push(`${label} contains ${name}`);
  }
}
const readmePath = resolve(frontendRoot, "..", "README.md");
const readme = readFileSync(readmePath, "utf8");
if (readme.includes("\uFFFD")) failures.push("README.md contains UTF-8 replacement characters");
for (const phrase of ["云端 API 地址", "云任务", "任务镜像", "登录后"]) {
  if (readme.includes(phrase)) failures.push(`README.md contains legacy product copy: ${phrase}`);
}
for (const path of productionFiles) {
  if (readFileSync(path, "utf8").includes("\uFFFD")) {
    failures.push(`${relative(frontendRoot, path)} contains UTF-8 replacement characters`);
  }
}
if (failures.length > 0) {
  process.stderr.write(`Runtime boundary failures:\n- ${failures.join("\n- ")}\n`);
  process.exit(1);
}

assert.ok(productionFiles.length >= 3, "expected main, app, and bridge modules");
process.stdout.write(`Runtime boundaries passed (${productionFiles.length} files checked).\n`);
