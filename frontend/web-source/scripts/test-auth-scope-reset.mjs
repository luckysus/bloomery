import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const workspacePath = path.join(root, "src", "hooks", "useResultWorkspace.ts");
const controllerPath = path.join(root, "src", "hooks", "useRagAppController.ts");

const workspace = fs.readFileSync(workspacePath, "utf8");
const controller = fs.readFileSync(controllerPath, "utf8");

assert.match(workspace, /authScopeKey:\s*string;/, "useResultWorkspace must accept an auth scope key");
assert.match(workspace, /setOverviewData\(null\);[\s\S]*void fetchOverview\(\);/, "overview data must reset before refetching for a new auth scope");
assert.match(workspace, /}, \[authScopeKey, fetchOverview\]\);/, "overview refetch effect must depend on auth scope changes");

assert.match(controller, /const authScopeKey = isAuthenticated \? authUser\?\.username \?\? "" : "";/, "controller must derive a stable auth scope key");
assert.match(controller, /authScopeKey={authScopeKey}|authScopeKey,/, "controller must pass auth scope into retrieval workspace");
assert.match(controller, /setData\(null\);[\s\S]*setQuery\(""\);/, "controller must clear retrieval results and query when auth scope changes");

console.log("auth scope reset self-test passed");
