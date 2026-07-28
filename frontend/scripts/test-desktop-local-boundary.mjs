import fs from "node:fs";
import path from "node:path";

const profilePath = new URL("../src/hooks/useProfileSettings.ts", import.meta.url);
const source = fs.readFileSync(profilePath, "utf8");
const agentRuntimeSource = fs.readFileSync(new URL("../src/hooks/useAgentRuntime.ts", import.meta.url), "utf8");
const searchModeSource = fs.readFileSync(new URL("../src/hooks/useSearchMode.ts", import.meta.url), "utf8");
const agentConversationsSource = fs.readFileSync(new URL("../src/hooks/useAgentConversations.ts", import.meta.url), "utf8");
const desktopMemoryPageSource = fs.readFileSync(new URL("../src/desktop/DesktopMemoryPage.tsx", import.meta.url), "utf8");
const searchServiceSource = fs.readFileSync(new URL("../src/services/search.ts", import.meta.url), "utf8");
const trainingServiceSource = fs.readFileSync(new URL("../src/services/training.ts", import.meta.url), "utf8");
const optimizerServiceSource = fs.readFileSync(new URL("../src/services/optimizer.ts", import.meta.url), "utf8");
const literatureServiceSource = fs.readFileSync(new URL("../src/services/literature.ts", import.meta.url), "utf8");
const labServiceSource = fs.readFileSync(new URL("../src/services/labService.ts", import.meta.url), "utf8");
const localAgentSource = fs.readFileSync(new URL("../../src-tauri/src/local_agent.rs", import.meta.url), "utf8");
const cloudTasksSource = fs.readFileSync(new URL("../../src-tauri/src/cloud_tasks.rs", import.meta.url), "utf8");

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function sliceBetween(text, startNeedle, endNeedle) {
  const start = text.indexOf(startNeedle);
  assert(start !== -1, `Missing source marker: ${startNeedle}`);
  const end = text.indexOf(endNeedle, start);
  assert(end !== -1, `Missing source marker: ${endNeedle}`);
  return text.slice(start, end);
}

function listSourceFiles(dirUrl) {
  const dir = dirUrl.pathname.replace(/^\/([A-Za-z]:\/)/, "$1");
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...listSourceFiles(new URL(`${entry.name}/`, dirUrl)));
    } else if (/\.(ts|tsx)$/.test(entry.name)) {
      results.push(fullPath);
    }
  }
  return results;
}

function assertNoTauriCloudLlmConfigSync() {
  const tauriBlocks = [...source.matchAll(/if \(isTauriRuntime\(\)\) \{([\s\S]*?)\n      \}/g)].map((match) => match[1]);
  const offenders = tauriBlocks.filter((block) => block.includes("saveUserLlmConfigRequest"));
  if (offenders.length) {
    throw new Error("Tauri LLM config branch must not sync model/API key settings to the cloud backend.");
  }
}

function assertDesktopModelListDoesNotUploadApiKey() {
  const loadModelsStart = source.indexOf("const loadLlmModels = useCallback");
  const loadModelsEnd = source.indexOf("const loadLlmConfig = useCallback", loadModelsStart);
  const loadModels = source.slice(loadModelsStart, loadModelsEnd);
  if (!loadModels.includes("isTauriRuntime()")) {
    throw new Error("Desktop model list must have an explicit Tauri branch.");
  }
  const tauriStart = loadModels.indexOf("if (isTauriRuntime())");
  const cloudModelCall = loadModels.indexOf("postUserLlmModels");
  const tauriBranch = loadModels.slice(tauriStart, cloudModelCall);
  if (tauriStart === -1 || cloudModelCall === -1 || !tauriBranch.includes("return;")) {
    throw new Error("Desktop model list must not send local API keys through the cloud backend.");
  }
}

function assertDesktopSourceDoesNotDirectFetchCloudQa() {
  const forbidden = [
    { pattern: /fetch\s*\(/, message: "Desktop-only source must call cloud through Tauri commands, not fetch()." },
    { pattern: /\/api\/ask|\/api\/agent\/(?:chat|stream|conversations)/, message: "Desktop-only source must not call cloud Q&A/agent endpoints." },
  ];
  for (const file of listSourceFiles(new URL("../src/desktop/", import.meta.url))) {
    const text = fs.readFileSync(file, "utf8");
    for (const item of forbidden) {
      if (item.pattern.test(text)) {
        throw new Error(`${item.message} Offender: ${file}`);
      }
    }
  }
}

function assertTauriAgentUsesLocalRuntimeOnly() {
  const runAgent = sliceBetween(agentRuntimeSource, "const runAgent = useCallback", "const handleAgentSubmit");
  assert(
    !runAgent.includes("isTauriRuntime() && confirmedActionIds.length === 0"),
    "Tauri agent branch must handle every desktop agent run locally, including confirmation mistakes.",
  );

  const streamIndex = runAgent.indexOf("/api/agent/stream");
  assert(streamIndex !== -1, "Missing web /api/agent/stream fallback marker.");
  const localBranchStart = runAgent.indexOf("if (isTauriRuntime())");
  assert(localBranchStart !== -1 && localBranchStart < streamIndex, "Missing Tauri branch before web agent stream.");
  const localBranch = runAgent.slice(localBranchStart, streamIndex);
  assert(localBranch.includes("streamDesktopAgent"), "Tauri agent branch must call streamDesktopAgent.");
  assert(localBranch.includes("return;"), "Tauri agent branch must return before web /api/agent/stream.");

  const chatFallbackIndex = runAgent.indexOf("/api/agent/chat", streamIndex);
  assert(chatFallbackIndex !== -1, "Missing web /api/agent/chat fallback marker.");
  const tauriCatchStart = runAgent.indexOf("if (isTauriRuntime())", streamIndex);
  assert(tauriCatchStart !== -1 && tauriCatchStart < chatFallbackIndex, "Missing Tauri catch guard before web agent chat fallback.");
  const tauriCatchBranch = runAgent.slice(tauriCatchStart, chatFallbackIndex);
  assert(tauriCatchBranch.includes("return;"), "Tauri agent errors must not fall through to web /api/agent/chat.");
}

function assertTauriAskUsesLocalRuntimeOnly() {
  const searchAskIndex = searchModeSource.indexOf("/api/ask");
  assert(searchAskIndex !== -1, "Missing web /api/ask marker in search mode.");
  const searchTauriStart = searchModeSource.indexOf('if ("__TAURI_INTERNALS__" in window)');
  assert(searchTauriStart !== -1 && searchTauriStart < searchAskIndex, "Search mode must check Tauri before web /api/ask.");
  const searchTauriBranch = searchModeSource.slice(searchTauriStart, searchAskIndex);
  assert(searchTauriBranch.includes("streamDesktopAsk"), "Tauri search/advice branch must call streamDesktopAsk.");
  assert(searchTauriBranch.includes("return;"), "Tauri search/advice branch must return before web /api/ask.");

  const retrievalFlow = sliceBetween(agentRuntimeSource, "const runAgentRetrievalOptimizationFlow = useCallback", "const runAgent = useCallback");
  const retrievalAskIndex = retrievalFlow.indexOf("/api/ask");
  assert(retrievalAskIndex !== -1, "Missing web /api/ask marker in retrieval optimization flow.");
  const retrievalTauriStart = retrievalFlow.indexOf("if (isTauriRuntime())");
  assert(retrievalTauriStart !== -1 && retrievalTauriStart < retrievalAskIndex, "Retrieval optimization flow must check Tauri before web /api/ask.");
  const retrievalTauriBranch = retrievalFlow.slice(retrievalTauriStart, retrievalAskIndex);
  assert(retrievalTauriBranch.includes("streamDesktopAsk"), "Tauri retrieval optimization branch must call streamDesktopAsk.");
  assert(retrievalTauriBranch.includes("} else {"), "Web /api/ask must stay inside the non-Tauri branch.");
}

function assertTauriConversationPersistenceUsesLocalRuntime() {
  const loadEffect = sliceBetween(agentConversationsSource, "async function loadRemoteAgentConversations", "const persistAgentConversation");
  const loadCloudIndex = loadEffect.indexOf("/api/agent/conversations?limit=80");
  assert(loadCloudIndex !== -1, "Missing web agent conversation list marker.");
  const loadTauriStart = loadEffect.indexOf("if (isTauriRuntime())");
  assert(loadTauriStart !== -1 && loadTauriStart < loadCloudIndex, "Tauri conversation list must load from SQLite before web conversation API.");
  const loadTauriBranch = loadEffect.slice(loadTauriStart, loadCloudIndex);
  assert(loadTauriBranch.includes("listDesktopConversations"), "Tauri conversation list must use desktop SQLite service.");
  assert(loadTauriBranch.includes("return;"), "Tauri conversation list must return before web conversation API.");

  const persistBlock = sliceBetween(agentConversationsSource, "const persistAgentConversation = useCallback", "const startNewAgentConversation");
  const remoteSaveIndex = persistBlock.indexOf("saveAgentConversationRemote");
  assert(remoteSaveIndex !== -1, "Missing web remote conversation save marker.");
  const persistTauriStart = persistBlock.indexOf("if (isTauriRuntime())");
  assert(persistTauriStart !== -1 && persistTauriStart < remoteSaveIndex, "Tauri conversation save must happen before remote save fallback.");
  const persistTauriBranch = persistBlock.slice(persistTauriStart, remoteSaveIndex);
  assert(persistTauriBranch.includes("saveDesktopConversationSnapshot"), "Tauri conversation save must use desktop SQLite snapshot.");
  assert(persistTauriBranch.includes("return;"), "Tauri conversation save must return before remote save fallback.");

  const pinBlock = sliceBetween(agentConversationsSource, "const toggleAgentConversationPin = useCallback", "const deleteAgentConversation");
  const pinCloudIndex = pinBlock.indexOf("/api/agent/conversations/");
  assert(pinCloudIndex !== -1, "Missing web conversation pin marker.");
  const pinTauriStart = pinBlock.indexOf("if (isTauriRuntime())");
  assert(pinTauriStart !== -1 && pinTauriStart < pinCloudIndex, "Tauri pin must update local SQLite before web conversation API.");
  const pinTauriBranch = pinBlock.slice(pinTauriStart, pinCloudIndex);
  assert(pinTauriBranch.includes("updateDesktopConversationPinned"), "Tauri pin must persist through desktop SQLite service.");
  assert(pinTauriBranch.includes("return;"), "Tauri pin must return before web conversation API.");
}

function assertTauriConversationStateDoesNotUseWebLocalCache() {
  const initialState = sliceBetween(agentConversationsSource, "const [agentConversations", "const [agentHistorySearchOpen");
  assert(
    initialState.includes("isTauriRuntime() ? [] : loadAgentConversations()"),
    "Tauri conversations must not initialize from the web/localStorage conversation cache.",
  );

  const persistBlock = sliceBetween(agentConversationsSource, "const persistAgentConversation = useCallback", "const startNewAgentConversation");
  assert(
    persistBlock.includes("if (!isTauriRuntime()) saveAgentConversations(next);"),
    "Tauri conversation persistence must not write the web/localStorage conversation cache.",
  );

  const updateBlock = sliceBetween(agentConversationsSource, "const updateAgentConversations = useCallback", "const toggleAgentConversationPin");
  assert(
    updateBlock.includes("if (!isTauriRuntime()) saveAgentConversations(next);"),
    "Tauri conversation updates must not write the web/localStorage conversation cache.",
  );
}

function assertTauriSharedCloudServicesUseRustProxy() {
  for (const [text, proxyNeedle, fetchNeedle, label] of [
    [searchServiceSource, 'desktopCloudTaskFetch("/api/overview")', 'fetch(`${API_BASE}/api/overview`', "overview"],
    [searchServiceSource, 'desktopCloudTaskFetch("/api/search"', 'fetch(`${API_BASE}/api/search`', "knowledge search"],
    [searchServiceSource, 'desktopCloudTaskFetch("/api/coil_match"', 'fetch(`${API_BASE}/api/coil_match`', "coil match"],
    [searchServiceSource, "desktopCloudDownloadFetch(path)", "fetch(`${API_BASE}${path}`", "export download"],
    [trainingServiceSource, 'desktopCloudTaskFetch("/api/training/start"', 'fetch(`${getApiBase()}/api/training/start`', "training start"],
    [trainingServiceSource, 'desktopCloudTaskFetch("/api/training/models")', 'fetch(`${getApiBase()}/api/training/models`', "training models"],
    [trainingServiceSource, "desktopCloudTaskFetch(`/api/training/status/", 'fetch(`${getApiBase()}/api/training/status/', "training status"],
    [trainingServiceSource, 'desktopCloudTaskFetch("/api/training/latest")', 'fetch(`${getApiBase()}/api/training/latest`', "training latest"],
    [trainingServiceSource, "desktopCloudTaskFetch(`/api/training/cancel/", 'fetch(`${getApiBase()}/api/training/cancel/', "training cancel"],
    [optimizerServiceSource, "desktopCloudTaskFetch(path)", 'fetch(`${getApiBase()}${path}`', "optimizer recent"],
    [optimizerServiceSource, 'desktopCloudTaskFetch("/api/optimize/logs")', 'fetch(`${getApiBase()}/api/optimize/logs`', "optimizer logs"],
    [optimizerServiceSource, 'desktopCloudTaskFetch("/api/optimize/cancel"', 'fetch(`${getApiBase()}/api/optimize/cancel`', "optimizer cancel"],
    [optimizerServiceSource, 'desktopCloudTaskFetch("/api/optimize"', 'fetch(`${getApiBase()}/api/optimize`', "optimizer run"],
    [literatureServiceSource, "desktopCloudTaskFetch(path, options)", "fetch(`${getApiBase()}${path}`", "literature json"],
    [literatureServiceSource, "desktopCloudBinaryFetch(`/api/literature/upload?", 'fetch(`${getApiBase()}/api/literature/upload?', "literature upload"],
    [labServiceSource, "desktopCloudTaskFetch(`/api/lab-service/status", 'fetch(`${API_BASE}/api/lab-service/status', "lab status"],
    [labServiceSource, 'desktopCloudTaskFetch("/api/lab-service/reconnect"', 'fetch(`${API_BASE}/api/lab-service/reconnect`', "lab reconnect"],
  ]) {
    const proxyIndex = text.indexOf(proxyNeedle);
    const fetchIndex = text.indexOf(fetchNeedle);
    assert(proxyIndex !== -1, `Missing Tauri Rust proxy marker for ${label}.`);
    assert(fetchIndex !== -1, `Missing web fetch fallback marker for ${label}.`);
    assert(proxyIndex < fetchIndex, `Tauri ${label} must use Rust cloud proxy before web fetch fallback.`);
  }

  const rawFallbackIndex = literatureServiceSource.indexOf("if (!canUseDesktopCloudTasks()) return { url:");
  const rawProxyIndex = literatureServiceSource.indexOf("desktopCloudDownloadFetch(path)");
  assert(rawFallbackIndex !== -1, "Missing web raw-PDF fallback guard for literature raw download.");
  assert(rawProxyIndex !== -1, "Missing Tauri raw-PDF Rust download proxy.");
  assert(rawFallbackIndex < rawProxyIndex, "Tauri raw-PDF download must stay after the non-Tauri fallback guard.");
}

function assertConfirmedCloudJobsMirrorBeforeCloudRequest() {
  for (const [startNeedle, endNeedle, label] of [
    ["async fn start_confirmed_training_job", "async fn start_confirmed_json_cloud_job", "training"],
    ["async fn start_confirmed_json_cloud_job", "fn save_submitting_cloud_job", "json cloud task"],
  ]) {
    const block = sliceBetween(localAgentSource, startNeedle, endNeedle);
    const mirrorIndex = block.indexOf("save_submitting_cloud_job");
    const sendIndex = block.indexOf(".send()");
    assert(mirrorIndex !== -1, `${label} confirmation must create a local submitting mirror.`);
    assert(sendIndex !== -1, `${label} confirmation must call a cloud task API.`);
    assert(mirrorIndex < sendIndex, `${label} confirmation must mirror locally before the cloud request.`);
    assert(
      block.includes("cloud_task_request_failed_outcome"),
      `${label} confirmation must keep a failed local mirror when the cloud request fails before submission.`,
    );
  }
}

function assertDesktopAgentPersistsUserBeforeCloudTool() {
  const chatBlock = sliceBetween(localAgentSource, "pub async fn desktop_agent_chat", "#[tauri::command]\npub async fn desktop_confirm_cloud_job");
  const cloudSearchIndex = chatBlock.indexOf("fetch_cloud_knowledge");
  assert(cloudSearchIndex !== -1, "Desktop agent must keep the cloud knowledge tool call visible in local_agent.rs.");
  const cloudJobReturnIndex = chatBlock.indexOf("return Ok(response);");
  assert(cloudJobReturnIndex !== -1 && cloudJobReturnIndex < cloudSearchIndex, "Desktop agent cloud-job confirmation branch marker is missing.");
  const beforeCloudSearch = chatBlock.slice(cloudJobReturnIndex, cloudSearchIndex);
  assert(
    beforeCloudSearch.includes('append_local_message(&db, &user_id, &conversation_id, "user", &message, None)?'),
    "Desktop agent must persist the user turn locally before calling cloud knowledge search.",
  );
}

function assertDesktopCloudTaskFailureMarksMirrorFailed() {
  const block = sliceBetween(cloudTasksSource, "pub async fn desktop_cloud_task_request", "#[tauri::command]\npub async fn desktop_cloud_binary_request");
  const sendIndex = block.indexOf(".send()");
  assert(sendIndex !== -1, "Desktop cloud task request must call the cloud backend.");
  const afterSend = block.slice(sendIndex);
  assert(afterSend.includes("Err(err)"), "Desktop cloud task request must handle send failures explicitly.");
  assert(afterSend.includes('\"failed\"'), "Desktop cloud task request failures must mark the local mirror as failed.");
  assert(afterSend.includes("mirror_cloud_job"), "Desktop cloud task request failures must update the local cloud job mirror.");
}

function assertMemorySuggestionsSaveLocally() {
  const acceptBlock = sliceBetween(desktopMemoryPageSource, "const handleAcceptSuggestion", "const setTags");
  assert(acceptBlock.includes("saveMemory"), "Desktop memory suggestions must be accepted into local SQLite, not only copied into the form.");
  assert(acceptBlock.includes("setSuggestions"), "Accepted desktop memory suggestions should disappear from the pending suggestion list.");
}

assertNoTauriCloudLlmConfigSync();
assertDesktopModelListDoesNotUploadApiKey();
assertDesktopSourceDoesNotDirectFetchCloudQa();
assertTauriAgentUsesLocalRuntimeOnly();
assertTauriAskUsesLocalRuntimeOnly();
assertTauriConversationPersistenceUsesLocalRuntime();
assertTauriConversationStateDoesNotUseWebLocalCache();
assertTauriSharedCloudServicesUseRustProxy();
assertConfirmedCloudJobsMirrorBeforeCloudRequest();
assertDesktopAgentPersistsUserBeforeCloudTool();
assertDesktopCloudTaskFailureMarksMirrorFailed();
assertMemorySuggestionsSaveLocally();
console.log("desktop local boundary checks passed");
