import type { ReactNode } from "react";
import { useChatControllerContext, type ChatControllerProps } from "./chatController";
import DesktopChatWorkspace from "./DesktopChatWorkspace";
import type { SectionId } from "../../app/navigation";

interface ChatPageProps {
  onOpenSection?: (section: SectionId) => void;
  renderLocalView?: (props: ChatControllerProps) => ReactNode;
}

export default function ChatPage({ onOpenSection, renderLocalView }: ChatPageProps = {}) {
  const controller = useChatControllerContext();
  if (renderLocalView) return renderLocalView(controller);
  return <DesktopChatWorkspace {...controller} onOpenSection={onOpenSection} />;
}
