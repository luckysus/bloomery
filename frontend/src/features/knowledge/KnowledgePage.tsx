import KnowledgeBaseWizard from "./KnowledgeBaseWizard";
import useKnowledgePageController from "./useKnowledgePageController";

export default function KnowledgePage() {
  return <KnowledgeBaseWizard {...useKnowledgePageController()} />;
}
