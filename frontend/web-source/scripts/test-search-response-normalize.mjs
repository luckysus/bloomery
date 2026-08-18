import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const root = process.cwd();
const sourcePath = path.join(root, "src", "utils", "searchResponse.ts");
const source = fs.readFileSync(sourcePath, "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2020,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;

const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`;
const { normalizeSearchResponse, readSearchResponse } = await import(moduleUrl);

const normalized = normalizeSearchResponse({
  success: true,
  production_columns: null,
  production_records: undefined,
  advice_contexts: null,
  advice_standard_columns: null,
  advice_standard_records: null,
  literature_results: null,
  literature_images: null,
  experimental_images: null,
});

assert.equal(normalized.success, true);
assert.deepEqual(normalized.production_columns, []);
assert.deepEqual(normalized.production_records, []);
assert.deepEqual(normalized.advice_contexts, []);
assert.deepEqual(normalized.advice_standard_columns, []);
assert.deepEqual(normalized.advice_standard_records, []);
assert.deepEqual(normalized.literature_results, []);
assert.deepEqual(normalized.literature_images, []);
assert.deepEqual(normalized.experimental_images, []);

const failed = normalizeSearchResponse({ detail: "no permission" });
assert.equal(failed.success, false);
assert.equal(failed.error, "no permission");

const response = new Response(JSON.stringify({ success: true, literature_results: null }), {
  headers: { "content-type": "application/json" },
});
const parsed = await readSearchResponse(response);
assert.equal(parsed.success, true);
assert.deepEqual(parsed.literature_results, []);

const errorResponse = new Response(JSON.stringify({ detail: "server failed" }), {
  status: 500,
  headers: { "content-type": "application/json" },
});
const parsedError = await readSearchResponse(errorResponse);
assert.equal(parsedError.success, false);
assert.equal(parsedError.error, "server failed");

console.log("search response normalize self-test passed");
