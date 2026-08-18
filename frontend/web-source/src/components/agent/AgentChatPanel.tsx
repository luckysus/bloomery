import { useRef, type Dispatch, type RefObject, type SetStateAction } from "react";
import { AlertCircle, ArrowUp, BotMessageSquare, Check, ChevronDown, Copy, Globe, Mic, Pencil, Settings2, Sparkles, Square, X } from "lucide-react";
import AgentProgressBar, { type AgentProgressState } from "../../agent/AgentProgressBar";
import type { AgentMessage, AgentPendingConfirmation, AgentResponse } from "../../agent/types";
import type { AgentAttachment } from "../../hooks/useAgentRuntime";
import { useSpeechToText } from "../../hooks/useSpeechToText";
import AgentConfirmDialog from "../../agent/AgentConfirmDialog";
import AgentFeedback from "../../agent/AgentFeedback";
import AgentRecommendationCard from "../../agent/AgentRecommendationCard";
import AgentTurnNavigator from "./AgentTurnNavigator.tsx";

interface ChatModelInfo {
  id: string;
  name: string;
  provider?: string;
}

interface AgentChatPanelProps {
  messagesRef: RefObject<HTMLDivElement>;
  messagesEndRef: RefObject<HTMLDivElement>;
  messages: AgentMessage[];
  loading: boolean;
  streaming: boolean;
  error: string;
  progress: AgentProgressState;
  response: AgentResponse | null;
  sessionId: string;
  query: string;
  editingMessageIndex: number | null;
  editingMessageText: string;
  copiedMessageIndex: number | null;
  modelMenuOpen: boolean;
  availableChatModels: ChatModelInfo[];
  currentChatModelName: string;
  activeModelDisplayName: string;
  webSearchEnabled: boolean;
  onToggleWebSearch: () => void;
  attachments: AgentAttachment[];
  setAttachments: Dispatch<SetStateAction<AgentAttachment[]>>;
  setQuery: (value: string) => void;
  setEditingMessageText: (value: string) => void;
  setModelMenuOpen: (updater: boolean | ((open: boolean) => boolean)) => void;
  onCancelEdit: () => void;
  onSubmitEditedMessage: () => void;
  onCopyMessage: (content: string, index: number) => void;
  onBeginEditMessage: (content: string, index: number) => void;
  onOpenOptimizer: () => void;
  onFeedback: (messageIndex: number, rating: "up" | "down", reason?: string) => void;
  onConfirmAction: (item: AgentPendingConfirmation, approved: boolean) => void;
  onSubmit: () => void;
  onStop: () => void;
  onSwitchModel: (modelId: string) => void;
  renderAnswer: (message: AgentMessage) => React.ReactNode;
}

export default function AgentChatPanel({
  messagesRef,
  messagesEndRef,
  messages,
  loading,
  streaming,
  error,
  progress,
  response,
  sessionId,
  query,
  editingMessageIndex,
  editingMessageText,
  copiedMessageIndex,
  modelMenuOpen,
  availableChatModels,
  currentChatModelName,
  activeModelDisplayName,
  webSearchEnabled,
  onToggleWebSearch,
  attachments,
  setAttachments,
  setQuery,
  setEditingMessageText,
  setModelMenuOpen,
  onCancelEdit,
  onSubmitEditedMessage,
  onCopyMessage,
  onBeginEditMessage,
  onOpenOptimizer,
  onFeedback,
  onConfirmAction,
  onSubmit,
  onStop,
  onSwitchModel,
  renderAnswer,
}: AgentChatPanelProps) {

  // 语音听写：录音开始前记录当前输入作为基线，识别文本覆盖式追加在其后。
  const dictationBaseRef = useRef("");
  const speech = useSpeechToText({
    onText: (text) => setQuery(dictationBaseRef.current + text),
  });
  const handleMicToggle = () => {
    if (!speech.recording) {
      const base = query.replace(/\s+$/, "");
      dictationBaseRef.current = base ? `${base} ` : "";
    }
    speech.toggle();
  };

  const handleFilesSelected = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    const images = Array.from(fileList).filter((file) => file.type.startsWith("image/"));
    const read = (file: File) =>
      new Promise<AgentAttachment | null>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => {
          const result = String(reader.result || "");
          const base64 = result.includes(",") ? result.slice(result.indexOf(",") + 1) : result;
          resolve(base64 ? { data: base64, mime: file.type || "image/png", name: file.name || "image" } : null);
        };
        reader.onerror = () => resolve(null);
        reader.readAsDataURL(file);
      });
    const results = (await Promise.all(images.map(read))).filter((item): item is AgentAttachment => item !== null);
    if (results.length > 0) {
      setAttachments((prev) => [...prev, ...results]);
    }
  };

  const removeAttachment = (index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <section className="flex-1 min-h-0 overflow-hidden bg-[#fbf7ef] [--agent-turn-gutter:clamp(88px,10vw,220px)] max-md:[--agent-turn-gutter:0px]">
      <div className="h-full flex flex-col">
        <div className="relative flex-1 min-h-0">
          <div ref={messagesRef} className="h-full overflow-y-auto overflow-x-hidden px-6 pt-5 max-md:px-3">
            <div className="grid min-h-full grid-cols-[var(--agent-turn-gutter)_minmax(0,1fr)_var(--agent-turn-gutter)] max-md:grid-cols-1">
              <div className="col-start-2 min-w-0 space-y-3 max-md:col-start-1">
          {messages.length === 0 && !loading && !error && (
            <div className="flex h-full min-h-[360px] flex-col items-center justify-center text-center">
              <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl border border-[#eadfd2] bg-[#fffaf3] shadow-[0_16px_34px_rgba(72,52,38,0.10)]">
                <Sparkles size={28} className="text-[#cc785c]" />
              </div>
              <h3 className="mb-1 text-lg font-semibold text-[#2b2118]">开始智能体对话</h3>
              <p className="max-w-xl text-sm leading-relaxed text-[#7d7065]">
                在底部输入问题或目标，智能体会结合生产数据、工艺标准、文献证据和评测结果给出回答。
              </p>
            </div>
          )}

          {messages.map((message, index) => (
            message.role === "user" ? (
              <UserMessage
                key={index}
                message={message}
                index={index}
                loading={loading}
                editingMessageIndex={editingMessageIndex}
                editingMessageText={editingMessageText}
                copiedMessageIndex={copiedMessageIndex}
                setEditingMessageText={setEditingMessageText}
                onCancelEdit={onCancelEdit}
                onSubmitEditedMessage={onSubmitEditedMessage}
                onCopyMessage={onCopyMessage}
                onBeginEditMessage={onBeginEditMessage}
              />
            ) : (
              <AgentMessageBlock
                key={index}
                message={message}
                index={index}
                messagesLength={messages.length}
                streaming={streaming}
                loading={loading}
                progress={progress}
                sessionId={sessionId}
                onOpenOptimizer={onOpenOptimizer}
                onFeedback={onFeedback}
                renderAnswer={renderAnswer}
              />
            )
          ))}

          {progress.active && progress.mode === "workflow" && messages[messages.length - 1]?.role === "user" && (
            <AgentProgressBar progress={progress} />
          )}

          {loading && messages[messages.length - 1]?.role === "user" && (
            <div className="px-2 py-5">
              <div className="flex h-7 items-center gap-1.5">
                    <span className="h-2 w-2 animate-bounce rounded-full bg-[#cc785c] [animation-delay:-0.24s]" />
                    <span className="h-2 w-2 animate-bounce rounded-full bg-[#cc785c] [animation-delay:-0.12s]" />
                    <span className="h-2 w-2 animate-bounce rounded-full bg-[#cc785c]" />
              </div>
            </div>
          )}

          {error && (
            <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-base text-red-600 shadow-sm">
              <AlertCircle size={16} className="inline mr-1.5 -mt-0.5" />
              {error}
            </div>
          )}

          {response?.follow_up_questions?.length ? (
            <div className="rounded-xl border border-blue-200 bg-blue-50 p-4 shadow-sm">
              <div className="mb-2 flex items-center gap-2 text-base font-semibold text-blue-800">
                <BotMessageSquare size={16} />
                需要补充的信息
              </div>
              <div className="grid gap-2 md:grid-cols-2">
                {response.follow_up_questions.map((question, index) => (
                  <button
                    key={index}
                    onClick={() => setQuery(question)}
                    className="rounded-lg border border-blue-200 bg-white px-3 py-2 text-left text-base leading-relaxed text-blue-700 transition-colors hover:bg-blue-50"
                  >
                    {question}
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          {response?.recommendations?.length ? (
            <div>
              <div className="mb-2 flex items-center gap-2 text-sm font-semibold uppercase tracking-widest text-slate-500">
                <Sparkles size={16} />
                推荐方案
              </div>
              <div className="grid gap-3 xl:grid-cols-2">
                {response.recommendations.map((item, index) => (
                  <AgentRecommendationCard key={`${item.title}-${index}`} item={item} />
                ))}
              </div>
            </div>
          ) : null}

          {response?.pending_confirmations?.length ? (
            <AgentConfirmDialog
              confirmations={response.pending_confirmations}
              onConfirm={onConfirmAction}
            />
          ) : null}

                <div ref={messagesEndRef} className="h-px" />
              </div>
            </div>
          </div>
          <AgentTurnNavigator
            messages={messages}
            scrollContainerRef={messagesRef}
          />
        </div>

        <div className="shrink-0 border-t border-[#eadfd2] bg-[#fbf7ef]/95 px-6 py-3 max-md:px-3 max-md:pb-[calc(0.75rem+env(safe-area-inset-bottom))]">
          <div className="grid grid-cols-[var(--agent-turn-gutter)_minmax(0,1fr)_var(--agent-turn-gutter)] max-md:grid-cols-1">
            <div className="col-start-2 min-w-0 max-md:col-start-1">
              <div className="rounded-2xl border border-[#e3d7ca] bg-[#fffaf3] p-2 shadow-[0_12px_30px_rgba(72,52,38,0.08)] transition-all duration-200 focus-within:border-[#cc785c]/45 focus-within:ring-4 focus-within:ring-[#cc785c]/10">
            {attachments.length > 0 && (
              <div className="mb-2 flex flex-wrap gap-2 px-1">
                {attachments.map((item, index) => (
                  <div key={`${item.name}-${index}`} className="group/att relative h-16 w-16 overflow-hidden rounded-lg border border-[#e3d7ca] bg-[#f7efe5]">
                    <img
                      src={`data:${item.mime};base64,${item.data}`}
                      alt={item.name}
                      className="h-full w-full object-cover"
                    />
                    <button
                      type="button"
                      onClick={() => removeAttachment(index)}
                      className="absolute right-0.5 top-0.5 flex h-5 w-5 items-center justify-center rounded-full bg-black/55 text-white opacity-0 transition-opacity group-hover/att:opacity-100"
                      aria-label="移除图片"
                      title="移除图片"
                    >
                      <X size={12} strokeWidth={2.6} />
                    </button>
                  </div>
                ))}
              </div>
            )}
            <textarea
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  onSubmit();
                }
              }}
              onPaste={(event) => {
                const files = event.clipboardData?.files;
                if (files && files.length > 0 && Array.from(files).some((file) => file.type.startsWith("image/"))) {
                  event.preventDefault();
                  void handleFilesSelected(files);
                }
              }}
              rows={2}
              className="max-h-32 min-h-[44px] w-full resize-none bg-transparent px-3 py-2 text-base leading-relaxed text-[#2b2118] outline-none placeholder:text-[#a39384]"
            />
            <div className="mt-1 flex items-center justify-between gap-2">
              <div className="flex shrink-0 items-center gap-2">
              <button
                type="button"
                onClick={onToggleWebSearch}
                disabled={loading || streaming}
                title="开启后，回答会结合实时联网搜索结果"
                className={`flex h-9 shrink-0 items-center gap-1.5 rounded-full border px-3 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
                  webSearchEnabled
                    ? "border-[#cc785c] bg-[#cc785c]/25 text-[#a85434]"
                    : "border-[#e3d7ca] bg-transparent text-[#6f6258] hover:bg-[#f7efe5] hover:text-[#2b2118]"
                }`}
              >
                <Globe size={16} />
                <span className="max-md:hidden">智能搜索</span>
              </button>
              <button
                type="button"
                onClick={handleMicToggle}
                disabled={loading || streaming || speech.available === false}
                title={
                  speech.available === false
                    ? "语音识别未配置"
                    : speech.recording
                      ? "聆听中，点击停止"
                      : speech.error || "语音输入"
                }
                className={`flex h-9 shrink-0 items-center gap-1.5 rounded-full border px-3 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
                  speech.recording
                    ? "animate-pulse border-[#cc785c] bg-[#cc785c] text-white"
                    : "border-[#e3d7ca] bg-transparent text-[#6f6258] hover:bg-[#f7efe5] hover:text-[#2b2118]"
                }`}
              >
                <Mic size={16} />
                <span className="max-md:hidden">{speech.recording ? "聆听中" : "语音"}</span>
              </button>
            </div>
            <div className="relative flex shrink-0 items-center gap-2">
                {modelMenuOpen && (
                  <div
                    className="absolute bottom-12 right-12 z-30 w-[min(460px,calc(100vw-64px))] overflow-hidden rounded-2xl border border-[#e3d7ca] bg-[#fffaf3] py-2 text-[#2b2118] shadow-2xl shadow-[#4c3425]/15 max-md:right-0 max-md:w-[min(460px,calc(100vw-24px))]"
                    onMouseDown={(event) => event.stopPropagation()}
                  >
                    <div className="max-h-[320px] overflow-y-auto px-1.5">
                      {availableChatModels.map((model) => {
                        const modelId = model.id;
                        const modelName = model.name || model.id;
                        const active = modelId === currentChatModelName;
                        return (
                          <button
                            key={modelId}
                            type="button"
                            onClick={() => {
                              setModelMenuOpen(false);
                              onSwitchModel(modelId);
                            }}
                            title={modelId}
                            className={`flex w-full items-start gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${
                              active ? "bg-[#f0e5da] text-[#2b2118]" : "hover:bg-[#f7efe5]"
                            }`}
                          >
                            <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-[#2b2118]">
                              {active && <Check size={16} strokeWidth={2.4} />}
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="block break-all text-sm font-semibold leading-snug">{modelId}</span>
                              <span className="mt-0.5 block truncate text-xs text-[#8a7668]">
                                {[modelName !== modelId ? modelName : "", model.provider].filter(Boolean).join(" / ")}
                              </span>
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                )}
                <button
                  type="button"
                  onClick={() => setModelMenuOpen((open) => !open)}
                  disabled={loading || streaming}
                  title="切换当前对话模型"
                  className="flex h-10 shrink-0 items-center justify-end gap-1.5 bg-transparent px-1 text-sm font-medium text-[#6f6258] transition-colors hover:text-[#2b2118] disabled:cursor-not-allowed disabled:opacity-70"
                >
                  <span className="whitespace-nowrap text-right max-md:max-w-[110px] max-md:truncate">{activeModelDisplayName}</span>
                  <ChevronDown size={15} className={`shrink-0 transition-transform duration-150 ${modelMenuOpen ? "rotate-180" : ""}`} />
                </button>
                <button
                  onClick={(loading || streaming) ? onStop : onSubmit}
                  disabled={!(loading || streaming) && !query.trim() && attachments.length === 0}
                  className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-full shadow-md transition-all duration-150 disabled:cursor-not-allowed disabled:opacity-50 ${
                    (loading || streaming)
                      ? "bg-[#fffaf3] text-[#2b2118] shadow-[#d8c9ba] ring-1 ring-[#e3d7ca] hover:bg-[#f7efe5]"
                      : "bg-[#6f5a48] text-white shadow-[#d8c9ba] hover:-translate-y-0.5 hover:bg-[#5d4939]"
                  }`}
                  aria-label={(loading || streaming) ? "停止生成" : "发送"}
                  title={(loading || streaming) ? "停止生成" : `发送，当前模型：${activeModelDisplayName}`}
                >
                  {(loading || streaming) ? <Square size={15} fill="currentColor" strokeWidth={2.4} /> : <ArrowUp size={20} strokeWidth={2.6} />}
                </button>
              </div>
            </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function UserMessage({
  message,
  index,
  loading,
  editingMessageIndex,
  editingMessageText,
  copiedMessageIndex,
  setEditingMessageText,
  onCancelEdit,
  onSubmitEditedMessage,
  onCopyMessage,
  onBeginEditMessage,
}: {
  message: AgentMessage;
  index: number;
  loading: boolean;
  editingMessageIndex: number | null;
  editingMessageText: string;
  copiedMessageIndex: number | null;
  setEditingMessageText: (value: string) => void;
  onCancelEdit: () => void;
  onSubmitEditedMessage: () => void;
  onCopyMessage: (content: string, index: number) => void;
  onBeginEditMessage: (content: string, index: number) => void;
}) {
  return (
    <div className="group flex justify-end" data-agent-user-turn={index}>
      {editingMessageIndex === index ? (
        <div className="w-full max-w-[760px] rounded-[28px] bg-slate-100 px-4 py-4 shadow-sm ring-1 ring-slate-200 max-md:max-w-full">
          <textarea
            autoFocus
            value={editingMessageText}
            onChange={(event) => setEditingMessageText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                onCancelEdit();
              }
              if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                onSubmitEditedMessage();
              }
            }}
            rows={3}
            className="min-h-[96px] w-full resize-none bg-transparent px-1 py-1 text-base leading-relaxed text-slate-900 outline-none"
          />
          <div className="mt-3 flex justify-end gap-2">
            <button
              type="button"
              onClick={onCancelEdit}
              className="h-9 rounded-full border border-slate-300 bg-white px-4 text-sm font-semibold text-slate-700 transition-colors hover:bg-slate-50"
            >
              取消
            </button>
            <button
              type="button"
              onClick={onSubmitEditedMessage}
              disabled={loading || !editingMessageText.trim()}
              className="h-9 rounded-full bg-slate-900 px-4 text-sm font-semibold text-white transition-colors hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-40"
            >
              发送
            </button>
          </div>
        </div>
      ) : (
        <div className="group/message flex max-w-[60%] flex-col items-end max-md:max-w-[86%]">
          <div className="rounded-2xl rounded-tr-md bg-[#4f382a] px-4 py-3 text-base leading-relaxed text-white shadow-sm">
            <div className="whitespace-pre-wrap break-words">
              {message.content}
            </div>
          </div>
          <div className="mt-1 flex h-8 items-center justify-end gap-1 opacity-0 transition-opacity duration-100 group-hover/message:opacity-100 focus-within:opacity-100">
            <button
              type="button"
              onClick={() => onCopyMessage(message.content, index)}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-900"
              aria-label={copiedMessageIndex === index ? "已复制" : "复制消息"}
              title={copiedMessageIndex === index ? "已复制" : "复制消息"}
            >
              {copiedMessageIndex === index ? <Check size={17} /> : <Copy size={17} />}
            </button>
            <button
              type="button"
              onClick={() => onBeginEditMessage(message.content, index)}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-900"
              aria-label="编辑消息"
              title="编辑消息"
            >
              <Pencil size={17} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function AgentMessageBlock({
  message,
  index,
  messagesLength,
  streaming,
  loading,
  progress,
  sessionId,
  onOpenOptimizer,
  onFeedback,
  renderAnswer,
}: {
  message: AgentMessage;
  index: number;
  messagesLength: number;
  streaming: boolean;
  loading: boolean;
  progress: AgentProgressState;
  sessionId: string;
  onOpenOptimizer: () => void;
  onFeedback: (messageIndex: number, rating: "up" | "down", reason?: string) => void;
  renderAnswer: (message: AgentMessage) => React.ReactNode;
}) {
  return (
    <>
      {progress.active && progress.mode === "workflow" && index === messagesLength - 1 && (
        <AgentProgressBar progress={progress} />
      )}
      <div className="px-2 py-4">
        <div className="ai-markdown-body w-full text-[17px] leading-relaxed text-slate-700">
          {renderAnswer(message)}
        </div>
        {message.action?.type === "process_optimization" && !(index === messagesLength - 1 && streaming) && (
          <div className="mt-4 flex justify-end">
            <button
              type="button"
              onClick={onOpenOptimizer}
              className="flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-base font-medium text-white shadow-sm transition-colors hover:bg-indigo-700"
            >
              <Settings2 className="h-5 w-5" />
              {message.action.label || "工艺寻优"}
            </button>
          </div>
        )}
        {message.response && !loading && !streaming && (
          <AgentFeedback
            messageId={`${sessionId || "session"}-${index}`}
            onFeedback={(rating, reason) => onFeedback(index, rating, reason)}
          />
        )}
      </div>
    </>
  );
}
