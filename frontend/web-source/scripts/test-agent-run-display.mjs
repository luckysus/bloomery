import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const root = process.cwd();
const sourcePath = path.join(root, "src", "agent", "agentRunDisplay.ts");
const source = fs.readFileSync(sourcePath, "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2020,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;

const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`;
const { stripInternalAgentBlocks } = await import(moduleUrl);

assert.equal(
  stripInternalAgentBlocks("结论 A\n<memory_compiler>{\"secret\":1}</memory_compiler>\n结论 B"),
  "结论 A\n结论 B",
);
assert.equal(
  stripInternalAgentBlocks("可展示\n```agent-internal\nhidden\n```\n继续"),
  "可展示\n继续",
);

console.log("agentRunDisplay self-test passed");
