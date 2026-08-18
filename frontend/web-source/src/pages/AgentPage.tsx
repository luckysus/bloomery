import AgentAnswerRenderer from "../components/agent/AgentAnswerRenderer";
import AgentChatPanel from "../components/agent/AgentChatPanel";

type AgentPageProps = Record<string, any>;

export default function AgentPage(props: AgentPageProps) {
  const {
    isAgentMode,
    isCoilMatchMode,
    agentMessagesRef,
    agentMessagesEndRef,
    agentMessages,
    agentLoading,
    agentStreaming,
    agentError,
    agentProgress,
    agentResponse,
    agentSessionId,
    query,
    editingAgentMessageIndex,
    editingAgentMessageText,
    copiedAgentMessageIndex,
    agentModelMenuOpen,
    availableChatModels,
    currentChatModelName,
    activeModelDisplayName,
    setQuery,
    setEditingAgentMessageText,
    setAgentModelMenuOpen,
    cancelEditAgentMessage,
    submitEditedAgentMessage,
    copyAgentMessage,
    beginEditAgentMessage,
    handleOpenOptimizer,
    handleAgentFeedback,
    confirmAgentAction,
    handleAgentSubmit,
    stopAIStreaming,
    switchAgentModel,
    webSearchEnabled,
    toggleWebSearch,
    agentAttachments,
    setAgentAttachments,
  } = props;

  if (!isAgentMode || isCoilMatchMode) return null;

  return (
    <AgentChatPanel
      messagesRef={agentMessagesRef}
      messagesEndRef={agentMessagesEndRef}
      messages={agentMessages}
      loading={agentLoading}
      streaming={agentStreaming}
      error={agentError}
      progress={agentProgress}
      response={agentResponse}
      sessionId={agentSessionId}
      query={query}
      editingMessageIndex={editingAgentMessageIndex}
      editingMessageText={editingAgentMessageText}
      copiedMessageIndex={copiedAgentMessageIndex}
      modelMenuOpen={agentModelMenuOpen}
      availableChatModels={availableChatModels}
      currentChatModelName={currentChatModelName}
      activeModelDisplayName={activeModelDisplayName}
      setQuery={setQuery}
      setEditingMessageText={setEditingAgentMessageText}
      setModelMenuOpen={setAgentModelMenuOpen}
      onCancelEdit={cancelEditAgentMessage}
      onSubmitEditedMessage={() => void submitEditedAgentMessage()}
      onCopyMessage={(content, index) => void copyAgentMessage(content, index)}
      onBeginEditMessage={beginEditAgentMessage}
      onOpenOptimizer={handleOpenOptimizer}
      onFeedback={(messageIndex, rating, reason) => void handleAgentFeedback(messageIndex, rating, reason)}
      onConfirmAction={confirmAgentAction}
      onSubmit={handleAgentSubmit}
      onStop={stopAIStreaming}
      onSwitchModel={(modelId) => void switchAgentModel(modelId)}
      webSearchEnabled={Boolean(webSearchEnabled)}
      onToggleWebSearch={() => toggleWebSearch?.()}
      attachments={agentAttachments ?? []}
      setAttachments={setAgentAttachments}
      renderAnswer={(message) => <AgentAnswerRenderer message={message} />}
    />
  );
}
