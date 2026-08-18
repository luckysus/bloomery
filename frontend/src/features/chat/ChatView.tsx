import WebAgentChatPanel, {
  type WebAgentChatPanelProps,
} from "./WebAgentChatPanel";

export type ChatViewProps = WebAgentChatPanelProps;

export default function ChatView(props: ChatViewProps) {
  return <WebAgentChatPanel {...props} />;
}
