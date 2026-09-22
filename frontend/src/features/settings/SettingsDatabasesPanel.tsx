import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import KnowledgeDatabasePanel from "./KnowledgeDatabasePanel";

export default function SettingsDatabasesPanel() {
  return (
    <>
      <KnowledgeDatabasePanel />
      <DatabaseConnectionsPanel />
    </>
  );
}
