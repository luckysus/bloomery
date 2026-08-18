import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const read = (...parts) => fs.readFileSync(path.join(root, ...parts), "utf8");
const controller = read("src", "hooks", "useRagAppController.ts");
const hook = read("src", "hooks", "useLiteratureUpload.ts");
const shell = read("src", "components", "layout", "RagAppShell.tsx");
const overlays = read("src", "components", "layout", "RagAppOverlays.tsx");
const wizard = read("src", "components", "KnowledgeBaseWizard.tsx");

const failures = [];
const check = (name, assertion) => {
  try {
    assertion();
  } catch (error) {
    failures.push(`${name}: ${error.message}`);
  }
};

const between = (source, startMarker, endMarker) => {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.ok(start >= 0 && end > start, `${startMarker} source block must exist`);
  return source.slice(start, end);
};

check("controller passes the auth scope", () => {
  assert.match(controller, /useLiteratureUpload\(authScopeKey\)/);
});

check("hook tracks folder load state, request scopes, and in-flight requests", () => {
  assert.match(hook, /export function useLiteratureUpload\(authScopeKey:\s*string\)/);
  assert.match(hook, /const \[litFoldersLoading, setLitFoldersLoading\] = useState\(false\);/);
  assert.match(hook, /const \[litFoldersLoaded, setLitFoldersLoaded\] = useState\(false\);/);
  assert.match(hook, /const literatureFoldersRequestSeqRef = useRef\(0\);/);
  assert.match(hook, /const literatureJobsRequestSeqRef = useRef\(0\);/);
  assert.match(hook, /const literatureFoldersInFlightRef = useRef\(false\);/);
  assert.match(hook, /const literatureJobsInFlightRef = useRef\(false\);/);
  assert.match(hook, /const currentAuthScopeKeyRef = useRef\(authScopeKey\);/);
  assert.match(hook, /requestScope !== currentAuthScopeKeyRef\.current/);
});

check("folder loading completes only after a successful response", () => {
  const foldersFetch = between(hook, "const fetchLitFolders", "const fetchLitJobs");
  const foldersUpdate = foldersFetch.indexOf("setLitFolders(data.folders || []);");
  const loadedUpdate = foldersFetch.indexOf("setLitFoldersLoaded(true);");
  const catchStart = foldersFetch.indexOf("} catch");
  const finallyStart = foldersFetch.indexOf("} finally");
  assert.ok(foldersUpdate >= 0 && loadedUpdate > foldersUpdate && loadedUpdate < catchStart, "loaded=true must stay in the successful try path");
  assert.doesNotMatch(foldersFetch.slice(finallyStart), /setLitFoldersLoaded\(true\)/, "finally must not mark a failed first load as loaded");
});

check("folder and job requests deduplicate before incrementing request sequence", () => {
  const foldersFetch = between(hook, "const fetchLitFolders", "const fetchLitJobs");
  const jobsFetch = between(hook, "const fetchLitJobs", "useEffect(() => {\n    selectedLiteratureFileRef");
  assert.match(foldersFetch, /const fetchLitFolders = useCallback\(async \(force = false\) =>/);
  assert.match(jobsFetch, /const fetchLitJobs = useCallback\(async \(force = false\) =>/);
  const foldersGuard = foldersFetch.indexOf("if (literatureFoldersInFlightRef.current && !force) return;");
  const foldersScopeGuard = foldersFetch.indexOf("if (!requestScope || requestScope !== currentAuthScopeKeyRef.current) return;");
  const foldersSeq = foldersFetch.indexOf("++literatureFoldersRequestSeqRef.current");
  const jobsGuard = jobsFetch.indexOf("if (literatureJobsInFlightRef.current && !force) return;");
  const jobsScopeGuard = jobsFetch.indexOf("if (!requestScope || requestScope !== currentAuthScopeKeyRef.current) return;");
  const jobsSeq = jobsFetch.indexOf("++literatureJobsRequestSeqRef.current");
  assert.ok(foldersScopeGuard >= 0 && foldersScopeGuard < foldersSeq, "folder scope guard must precede seq++");
  assert.ok(foldersGuard >= 0 && foldersGuard < foldersSeq, "folder in-flight guard must precede seq++");
  assert.ok(jobsScopeGuard >= 0 && jobsScopeGuard < jobsSeq, "job scope guard must precede seq++");
  assert.ok(jobsGuard >= 0 && jobsGuard < jobsSeq, "job in-flight guard must precede seq++");
  assert.match(foldersFetch, /requestSeq === literatureFoldersRequestSeqRef\.current && requestScope === currentAuthScopeKeyRef\.current[\s\S]*literatureFoldersInFlightRef\.current = false;/);
  assert.match(jobsFetch, /requestSeq === literatureJobsRequestSeqRef\.current && requestScope === currentAuthScopeKeyRef\.current[\s\S]*literatureJobsInFlightRef\.current = false;/);
});

check("mutations force immediate folder and job refreshes", () => {
  assert.equal((hook.match(/await fetchLitFolders\(true\);/g) || []).length, 5, "all five folder mutations must force refresh");
  assert.equal((hook.match(/await fetchLitJobs\(true\);/g) || []).length, 4, "all four job mutations must force refresh");

  const upload = between(hook, "const uploadKnowledgeFiles", "const renameKnowledgeFile");
  const renameFile = between(hook, "const renameKnowledgeFile", "const deleteKnowledgeFile");
  const deleteFile = between(hook, "const deleteKnowledgeFile", "const deleteKnowledgeFolder");
  const deleteFolder = between(hook, "const deleteKnowledgeFolder", "const mergeKnowledgeFolder");
  const mergeFolder = between(hook, "const mergeKnowledgeFolder", "const selectedFolderInfo");
  assert.match(upload, /await fetchLitFolders\(true\);/);
  assert.match(renameFile, /await fetchLitFolders\(true\);/);
  assert.match(deleteFile, /await fetchLitFolders\(true\);/);
  assert.match(deleteFolder, /await fetchLitFolders\(true\);[\s\S]*await fetchLitJobs\(true\);/);
  assert.match(mergeFolder, /await fetchLitFolders\(true\);/);

  const startProcessing = between(hook, "const startLitProcessing", "const uploadKnowledgeFiles");
  const confirmProcessing = between(hook, "const confirmKnowledgeProcessing", "const deleteLitJob");
  const deleteJob = between(hook, "const deleteLitJob", "useEffect(() => {\n    if (showLiterature)");
  assert.match(startProcessing, /await fetchLitJobs\(true\);/);
  assert.match(confirmProcessing, /await fetchLitJobs\(true\);/);
  assert.match(deleteJob, /await fetchLitJobs\(true\);/);
});

check("all asynchronous mutations reject stale auth scopes", () => {
  assert.match(hook, /const isCurrentAuthScope = useCallback\([\s\S]*scope === currentAuthScopeKeyRef\.current[\s\S]*, \[\]\);/);
  const mutations = [
    ["startLitProcessing", "const startLitProcessing", "const uploadKnowledgeFiles"],
    ["uploadKnowledgeFiles", "const uploadKnowledgeFiles", "const renameKnowledgeFile"],
    ["renameKnowledgeFile", "const renameKnowledgeFile", "const deleteKnowledgeFile"],
    ["deleteKnowledgeFile", "const deleteKnowledgeFile", "const deleteKnowledgeFolder"],
    ["deleteKnowledgeFolder", "const deleteKnowledgeFolder", "const mergeKnowledgeFolder"],
    ["mergeKnowledgeFolder", "const mergeKnowledgeFolder", "const selectedFolderInfo"],
    ["confirmKnowledgeProcessing", "const confirmKnowledgeProcessing", "const deleteLitJob"],
    ["deleteLitJob", "const deleteLitJob", "useEffect(() => {\n    if (showLiterature)"],
  ];
  for (const [name, start, end] of mutations) {
    const block = between(hook, start, end);
    assert.match(block, /const mutationScope = authScopeKey;/, `${name} must capture its auth scope`);
    assert.match(block, /if \(!isCurrentAuthScope\(mutationScope\)\) return;/, `${name} must reject a stale entry scope`);
    const awaits = (block.match(/\bawait\b/g) || []).length;
    const guardedAwaits = (block.match(/\bawait\b[^;]*;\s*if \(!isCurrentAuthScope\(mutationScope\)\) return;/g) || []).length;
    assert.equal(guardedAwaits, awaits, `${name} must guard immediately after every await`);
    if (/\} catch/.test(block)) {
      assert.match(block, /\} catch(?: \([^)]*\))? \{\s*if \(!isCurrentAuthScope\(mutationScope\)\) return;/, `${name} catch must ignore stale scopes`);
    }
  }
  const startProcessing = between(hook, "const startLitProcessing", "const uploadKnowledgeFiles");
  const upload = between(hook, "const uploadKnowledgeFiles", "const renameKnowledgeFile");
  assert.match(startProcessing, /\} finally \{\s*if \(!isCurrentAuthScope\(mutationScope\)\) return;\s*setLitLoading\(null\);/);
  assert.match(upload, /\} finally \{\s*if \(!isCurrentAuthScope\(mutationScope\)\) return;\s*setKnowledgeUploadBusy\(false\);/);
});

check("prefetch, overlay opening, and polling keep deduplicated reads", () => {
  const showEffect = between(hook, "useEffect(() => {\n    if (showLiterature)", "useEffect(() => {\n    if (!showLiterature");
  assert.match(hook, /void Promise\.all\(\[fetchLitFolders\(\), fetchLitJobs\(\)\]\);/);
  assert.match(showEffect, /fetchLitFolders\(\);[\s\S]*fetchLitJobs\(\);[\s\S]*setInterval\(fetchLitJobs, 3000\);/);
  assert.doesNotMatch(showEffect, /fetchLitFolders\(true\)|fetchLitJobs\(true\)/);
});

check("scope changes invalidate requests and prefetch in parallel", () => {
  assert.match(hook, /literatureFoldersRequestSeqRef\.current \+= 1;/);
  assert.match(hook, /literatureJobsRequestSeqRef\.current \+= 1;/);
  assert.match(hook, /literatureFilesRequestSeqRef\.current \+= 1;/);
  assert.match(hook, /literaturePreviewRequestSeqRef\.current \+= 1;/);
  const scopeReset = between(hook, "literatureFoldersRequestSeqRef.current += 1;", "const openKnowledgeWizard");
  assert.match(scopeReset, /literatureFoldersInFlightRef\.current = false;[\s\S]*literatureJobsInFlightRef\.current = false;/);
  assert.match(scopeReset, /setShowLiterature\(false\);[\s\S]*resetKnowledgeWizard\(\);[\s\S]*setLitFolders\(\[\]\);[\s\S]*setLitJobs\(\[\]\);/);
  assert.match(scopeReset, /setLiteratureFiles\(\[\]\);[\s\S]*selectedLiteratureFileRef\.current = "";[\s\S]*setLiteratureFilePreview\(null\);/);
  assert.match(scopeReset, /void Promise\.all\(\[fetchLitFolders\(\), fetchLitJobs\(\)\]\);/);
  const wizardReset = between(hook, "const resetKnowledgeWizard", "useEffect(() => {\n    literatureFoldersRequestSeqRef");
  assert.match(wizardReset, /setKnowledgeName\(""\);[\s\S]*setKnowledgeFolder\(""\);[\s\S]*setKnowledgeUploadedFiles\(\[\]\);/);
  assert.match(wizardReset, /setExpandedJobId\(null\);[\s\S]*setSelectedLiteratureFile\(""\);[\s\S]*setLiteratureFilePreview\(null\);/);
});

check("folder refresh keeps the previous list", () => {
  const foldersFetch = between(hook, "const fetchLitFolders", "const fetchLitJobs");
  assert.doesNotMatch(foldersFetch, /setLitFolders\(\[\]\)/);
});

check("all slow literature reads reject stale responses", () => {
  const foldersFetch = between(hook, "const fetchLitFolders", "const fetchLitJobs");
  const jobsFetch = between(hook, "const fetchLitJobs", "useEffect(() => {\n    selectedLiteratureFileRef");
  const filesFetch = between(hook, "const fetchLiteratureFiles", "const openKnowledgeCreate");
  const previewFetch = between(hook, "getLiteratureFilePreview", "useEffect(() => {\n    const requestScope = authScopeKey;\n    if (!requestScope");
  assert.match(foldersFetch, /requestSeq !== literatureFoldersRequestSeqRef\.current \|\| requestScope !== currentAuthScopeKeyRef\.current/);
  assert.match(jobsFetch, /requestSeq !== literatureJobsRequestSeqRef\.current \|\| requestScope !== currentAuthScopeKeyRef\.current/);
  assert.match(filesFetch, /requestSeq !== literatureFilesRequestSeqRef\.current \|\| requestScope !== currentAuthScopeKeyRef\.current/);
  assert.match(previewFetch, /!cancelled && requestSeq === literaturePreviewRequestSeqRef\.current && requestScope === currentAuthScopeKeyRef\.current/);
});

check("folder load state passes through shell and overlays", () => {
  assert.match(shell, /litFoldersLoading,[\s\S]*litFoldersLoaded,/);
  assert.match(shell, /litFoldersLoading=\{litFoldersLoading\}/);
  assert.match(shell, /litFoldersLoaded=\{litFoldersLoaded\}/);
  assert.match(overlays, /litFoldersLoading,[\s\S]*litFoldersLoaded,/);
  assert.match(overlays, /foldersLoading=\{litFoldersLoading\}/);
  assert.match(overlays, /foldersLoaded=\{litFoldersLoaded\}/);
});

check("folder retry passes through overlays and exposes accessible states", () => {
  assert.match(shell, /fetchLitFolders,/);
  assert.match(shell, /fetchLitFolders=\{fetchLitFolders\}/);
  assert.match(overlays, /fetchLitFolders,/);
  assert.match(overlays, /onRetryFolders=\{\(\) => void fetchLitFolders\(\)\}/);
  assert.match(wizard, /onRetryFolders:\s*\(\) => void;/);
  assert.match(wizard, /aria-busy=\{props\.foldersLoading\}/);
  assert.ok((wizard.match(/role="status"/g) || []).length >= 2, "loading and failure states must be announced");
  const busyContainer = wizard.indexOf("<div aria-busy={props.foldersLoading}");
  const loadingStatus = wizard.indexOf('role="status" className="sr-only"');
  assert.ok(loadingStatus >= 0 && loadingStatus < busyContainer, "loading status must stay outside the busy container");
  const loadingState = between(
    wizard,
    "!props.foldersLoaded && props.foldersLoading && props.folders.length === 0 ? (",
    ") : !props.foldersLoaded && !props.foldersLoading && props.folders.length === 0 ? (",
  );
  assert.match(loadingState, /aria-hidden="true"[\s\S]*Array\.from\(\{ length: 3 \}\)/);
  assert.doesNotMatch(loadingState, /role="status"|sr-only/);
  assert.match(loadingState, /h-16[^"\n]*animate-pulse[^"\n]*motion-reduce:animate-none/);
  assert.match(wizard, /h-5 w-10[^"\n]*animate-pulse[^"\n]*motion-reduce:animate-none/);
  assert.match(wizard, /onClick=\{props\.onRetryFolders\}[\s\S]*<RefreshCw/);
});

check("wizard shows the folder count only after loading succeeds", () => {
  const folderHeader = between(
    wizard,
    "<h3 className=",
    "!props.foldersLoaded && props.foldersLoading && props.folders.length === 0 && (",
  );
  assert.match(folderHeader, /!props\.foldersLoaded \? \(/);
  assert.doesNotMatch(folderHeader, /foldersLoading/);
  assert.match(folderHeader, /\) : \([\s\S]*\{props\.folders\.length\} 个/);
});

check("wizard shows loading placeholders before the empty state", () => {
  assert.match(wizard, /foldersLoading:\s*boolean;/);
  assert.match(wizard, /foldersLoaded:\s*boolean;/);
  assert.match(wizard, /disabled=\{!props\.foldersLoaded \|\| props\.folders\.length < 2\}/);
  assert.match(wizard, /h-5 w-10[^"\n]*animate-pulse/);
  assert.match(wizard, /Array\.from\(\{ length: 3 \}\)/);
  assert.match(wizard, /h-16[^"\n]*animate-pulse/);
  assert.match(wizard, /!props\.foldersLoaded \? \([\s\S]*h-5 w-10[^"\n]*animate-pulse/);
  const loadingBranch = wizard.indexOf("!props.foldersLoaded && props.foldersLoading && props.folders.length === 0");
  const failureBranch = wizard.indexOf("!props.foldersLoaded && !props.foldersLoading && props.folders.length === 0");
  const loadedEmptyBranch = wizard.indexOf("props.foldersLoaded && props.folders.length === 0");
  const emptyBranch = wizard.indexOf("暂无知识库");
  assert.ok(loadingBranch >= 0 && loadingBranch < failureBranch, "loading branch must precede the failure branch");
  assert.ok(failureBranch < loadedEmptyBranch && loadedEmptyBranch < emptyBranch, "failure must precede the loaded empty state");
  assert.doesNotMatch(wizard, /!props\.foldersLoaded \|\| props\.foldersLoading/);
});

if (failures.length > 0) {
  console.error(`literature prefetch contract failed (${failures.length}):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log("literature prefetch contract passed");
}
