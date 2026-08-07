import { useEffect, useState } from "react";
import {
  desktop,
  type BackgroundTask,
  type Conversation,
  type KnowledgeBaseRecord,
  type KnowledgeHealth,
} from "../../bridge/desktop";

export type WorkbenchOverviewSource = "conversations" | "knowledgeBases" | "backgroundTasks" | "knowledgeHealth";

export interface WorkbenchOverview {
  conversations: Conversation[];
  knowledgeBases: KnowledgeBaseRecord[];
  backgroundTasks: BackgroundTask[];
  health: KnowledgeHealth | null;
  failedSources: WorkbenchOverviewSource[];
  loading: boolean;
  refresh: () => void;
}

interface WorkbenchOverviewData {
  conversations: Conversation[];
  knowledgeBases: KnowledgeBaseRecord[];
  backgroundTasks: BackgroundTask[];
  health: KnowledgeHealth | null;
  failedSources: WorkbenchOverviewSource[];
}

const emptyOverview: WorkbenchOverviewData = {
  conversations: [],
  knowledgeBases: [],
  backgroundTasks: [],
  health: null,
  failedSources: [],
};

export function useWorkbenchOverview(enabled: boolean): WorkbenchOverview {
  const [revision, setRevision] = useState(0);
  const [overview, setOverview] = useState(emptyOverview);
  const [loading, setLoading] = useState(enabled);

  useEffect(() => {
    let mounted = true;
    if (!enabled) {
      setLoading(false);
      return () => {
        mounted = false;
      };
    }

    setLoading(true);
    void Promise.allSettled([
      desktop.listConversations(),
      desktop.listKnowledgeBases(),
      desktop.listBackgroundTasks(),
      desktop.getKnowledgeHealth(),
    ]).then(([conversations, knowledgeBases, backgroundTasks, health]) => {
      if (!mounted) return;

      const failedSources: WorkbenchOverviewSource[] = [];
      const nextConversations = conversations.status === "fulfilled" ? conversations.value : (failedSources.push("conversations"), []);
      const nextKnowledgeBases = knowledgeBases.status === "fulfilled" ? knowledgeBases.value : (failedSources.push("knowledgeBases"), []);
      const nextBackgroundTasks = backgroundTasks.status === "fulfilled" ? backgroundTasks.value : (failedSources.push("backgroundTasks"), []);
      const nextHealth = health.status === "fulfilled" ? health.value : (failedSources.push("knowledgeHealth"), null);

      setOverview({
        conversations: nextConversations,
        knowledgeBases: nextKnowledgeBases,
        backgroundTasks: nextBackgroundTasks,
        health: nextHealth,
        failedSources,
      });
      setLoading(false);
    });

    return () => {
      mounted = false;
    };
  }, [enabled, revision]);

  return {
    ...overview,
    loading,
    refresh: () => setRevision((current) => current + 1),
  };
}
