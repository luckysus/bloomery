import KnowledgeBaseWizard from "../components/KnowledgeBaseWizard";

type LiteraturePageProps = Record<string, any>;

export default function LiteraturePage(props: LiteraturePageProps) {
  return <KnowledgeBaseWizard {...(props as any)} />;
}
