import { useEffect, useState, type FormEvent } from "react";
import { useLocale } from "../../i18n/locale";
import {
  desktop,
  type BackgroundTask,
  type KnowledgeBaseDeleteImpact,
  type KnowledgeBaseRecord,
  type KnowledgeHealth,
  type SourceDocumentRecord,
} from "../../bridge/desktop";
import KnowledgeView from "./KnowledgeView";

const emptyHealth: KnowledgeHealth = {
  knowledge_base_count: 0,
  document_count: 0,
  active_document_count: 0,
  version_count: 0,
  chunk_count: 0,
  indexed_chunk_count: 0,
  active_task_count: 0,
};

interface RetrievalSetup {
  embeddingProfileId: string | null;
  mineruProfileId: string | null;
}

const emptyRetrieval: RetrievalSetup = { embeddingProfileId: null, mineruProfileId: null };

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function parseRetrievalSetup(value: string | null): RetrievalSetup {
  if (!value) return emptyRetrieval;
  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    return {
      embeddingProfileId: typeof parsed.embedding_profile_id === "string" ? parsed.embedding_profile_id : null,
      mineruProfileId: typeof parsed.mineru_profile_id === "string" ? parsed.mineru_profile_id : null,
    };
  } catch {
    return emptyRetrieval;
  }
}

export default function KnowledgePage() {
  const { t } = useLocale();
  const [bases, setBases] = useState<KnowledgeBaseRecord[]>([]);
  const [selectedBaseId, setSelectedBaseId] = useState<string | null>(null);
  const [documents, setDocuments] = useState<SourceDocumentRecord[]>([]);
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [health, setHealth] = useState<KnowledgeHealth>(emptyHealth);
  const [retrieval, setRetrieval] = useState<RetrievalSetup>(emptyRetrieval);
  const [newName, setNewName] = useState("");
  const [filePath, setFilePath] = useState("");
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteImpact, setDeleteImpact] = useState<KnowledgeBaseDeleteImpact | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadOverview = async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextBases, nextHealth, nextTasks, setting] = await Promise.all([
        desktop.listKnowledgeBases(),
        desktop.getKnowledgeHealth(),
        desktop.listBackgroundTasks(),
        desktop.getSetting("onboarding.retrieval"),
      ]);
      setBases(nextBases);
      setHealth(nextHealth);
      setTasks(nextTasks);
      setRetrieval(parseRetrievalSetup(setting));
      setSelectedBaseId((current) => current && nextBases.some((base) => base.id === current) ? current : nextBases[0]?.id ?? null);
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void loadOverview(); }, []);

  useEffect(() => {
    if (!selectedBaseId) {
      setDocuments([]);
      return;
    }
    let mounted = true;
    desktop.listKnowledgeDocuments(selectedBaseId).then((items) => {
      if (mounted) setDocuments(items);
    }).catch((cause) => {
      if (mounted) setError(errorMessage(cause, t("knowledgeError")));
    });
    return () => { mounted = false; };
  }, [selectedBaseId]);

  const createBase = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const created = await desktop.createKnowledgeBase(name);
      setBases((current) => [...current, created]);
      setSelectedBaseId(created.id);
      setNewName("");
      setHealth((current) => ({ ...current, knowledge_base_count: current.knowledge_base_count + 1 }));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const startRename = (base: KnowledgeBaseRecord) => {
    setRenameId(base.id);
    setRenameName(base.name);
    setError(null);
  };

  const saveRename = async () => {
    if (!renameId || !renameName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await desktop.renameKnowledgeBase(renameId, renameName.trim());
      setBases((current) => current.map((base) => base.id === updated.id ? updated : base));
      setRenameId(null);
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const requestDelete = async (base: KnowledgeBaseRecord) => {
    setBusy(true);
    setError(null);
    try {
      setDeleteImpact(await desktop.previewDeleteKnowledgeBase(base.id));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteImpact) return;
    setBusy(true);
    setError(null);
    try {
      await desktop.deleteKnowledgeBaseConfirmed(deleteImpact.knowledge_base_id);
      const remaining = bases.filter((base) => base.id !== deleteImpact.knowledge_base_id);
      setBases(remaining);
      setSelectedBaseId(remaining[0]?.id ?? null);
      setDeleteImpact(null);
      setHealth((current) => ({ ...current, knowledge_base_count: Math.max(0, current.knowledge_base_count - 1) }));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const importDocument = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedBaseId || !filePath.trim()) return;
    if (!retrieval.embeddingProfileId || !retrieval.mineruProfileId) {
      setError(t("setupRetrievalFirst"));
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await desktop.importLocalDocument({ source_path: filePath.trim(), knowledge_base: { mode: "existing", id: selectedBaseId }, mineru_profile_id: retrieval.mineruProfileId, embedding_profile_id: retrieval.embeddingProfileId, embedding_dimension: 1024 });
      setFilePath("");
      setNotice(t("importCreated"));
      await loadOverview();
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const chooseFile = async () => {
    setError(null);
    try {
      const selected = await desktop.openFileDialog({ directory: false, multiple: false, title: t("browseFile"), filters: [{ name: t("importLocalDocument"), extensions: ["pdf", "docx", "xlsx", "csv", "md", "txt", "html"] }] });
      if (typeof selected === "string") setFilePath(selected);
    } catch (cause) {
      setError(errorMessage(cause, t("filePickerError")));
    }
  };

  return <KnowledgeView
    bases={bases}
    selectedBaseId={selectedBaseId}
    documents={documents}
    tasks={tasks}
    health={health}
    newName={newName}
    filePath={filePath}
    renameId={renameId}
    renameName={renameName}
    deleteImpact={deleteImpact}
    loading={loading}
    busy={busy}
    error={error}
    notice={notice}
    onRefresh={() => void loadOverview()}
    onCreateBase={createBase}
    onNewNameChange={setNewName}
    onSelectBase={setSelectedBaseId}
    onStartRename={startRename}
    onRenameNameChange={setRenameName}
    onSaveRename={() => void saveRename()}
    onCancelRename={() => setRenameId(null)}
    onRequestDelete={(base) => void requestDelete(base)}
    onCancelDelete={() => setDeleteImpact(null)}
    onConfirmDelete={() => void confirmDelete()}
    onImportDocument={importDocument}
    onFilePathChange={setFilePath}
    onChooseFile={() => void chooseFile()}
  />;
}
