export interface AgentPlanStep {
  step_id: string;
  title: string;
  description?: string;
  tool_name?: string | null;
  status: string;
  summary?: string;
  error?: string | null;
}

export interface AgentToolCall {
  call_id: string;
  action_id: string;
  tool_name: string;
  user_id?: number;
  title?: string;
  permission: "auto" | "confirm" | "danger" | string;
  arguments: Record<string, unknown>;
  status: string;
  started_at?: string | null;
  ended_at?: string | null;
  duration_ms?: number | null;
  result_summary?: string;
  error?: string | null;
  cache_hit?: boolean;
  retry_count?: number;
  artifact_ref?: string;
  remote_job_id?: string;
  namespace?: string;
  metadata?: Record<string, unknown>;
}

export interface AgentEvidence {
  evidence_id: string;
  type: string;
  title: string;
  source_id?: string;
  source_label?: string;
  content?: string;
  score?: number | null;
  evidence_level?: "direct" | "indirect" | "empirical" | "predicted" | "generated" | string;
  metadata?: Record<string, unknown>;
}

export interface AgentRecommendation {
  title: string;
  category?: string;
  summary?: string;
  details?: Record<string, unknown>;
  risks?: string[];
  evidence_ids?: string[];
}

export interface AgentVerification {
  confidence: "high" | "medium" | "low" | string;
  citation_count: number;
  missing_citations: string[];
  numeric_warnings: string[];
  unsupported_claims: string[];
  summary: string;
}

export interface AgentMemory {
  session_id: string;
  target_yield?: number | null;
  target_tensile?: number | null;
  target_elongation?: number | null;
  steel_mark?: string;
  steel_grade?: string;
  last_intent?: string;
  last_filters?: Record<string, unknown>;
  last_tool_summaries?: string[];
  constraints?: Record<string, unknown>;
  notes?: string[];
  updated_at?: string;
}

export interface AgentIntent {
  intent_type: string;
  domain: string;
  risk_level: string;
  needs_evidence: boolean;
  needs_tools: string[];
  missing_slots: string[];
  answer_policy: string;
  reason: string;
  /** 鎰忓浘缃俊搴?0-1 */
  confidence?: number;
  legacy_flags?: Record<string, boolean>;
}

export interface AgentWorkflowTrace {
  route: string;
  tools_selected: string[];
  tools_skipped: string[];
  evidence_policy: string;
  answer_policy: string;
  model_provider: string;
  model_name: string;
  notes: string[];
}

export interface AgentWorkflowNode {
  node_id: string;
  type: string;
  title: string;
  description?: string;
  status: string;
  started_at?: string | null;
  ended_at?: string | null;
  duration_ms?: number | null;
  inputs_summary?: string;
  outputs_summary?: string;
  tool_name?: string | null;
  evidence_count?: number;
  error?: string | null;
  metadata?: Record<string, unknown>;
}

export interface AgentWorkflowEdge {
  source: string;
  target: string;
  label?: string;
}

export interface AgentWorkflowRun {
  run_id: string;
  state: string;
  nodes: AgentWorkflowNode[];
  edges: AgentWorkflowEdge[];
  events: Array<Record<string, unknown>>;
  summary?: string;
  started_at?: string;
  ended_at?: string | null;
}

export interface AgentModelConfigSummary {
  provider: string;
  base_url: string;
  model_name: string;
  has_api_key: boolean;
}

export interface AgentPendingConfirmation {
  action_id: string;
  tool_name: string;
  title: string;
  permission: string;
  arguments: Record<string, unknown>;
  warning?: string;
}

export interface AgentResponse {
  run_id: string;
  session_id: string;
  status: string;
  answer: string;
  follow_up_questions: string[];
  plan_steps: AgentPlanStep[];
  tool_calls: AgentToolCall[];
  evidence: AgentEvidence[];
  recommendations: AgentRecommendation[];
  verification: AgentVerification;
  memory: AgentMemory;
  pending_confirmations: AgentPendingConfirmation[];
  intent: AgentIntent;
  workflow_trace: AgentWorkflowTrace;
  workflow: AgentWorkflowRun;
  llm_config: AgentModelConfigSummary;
}

export interface AgentWebSource {
  index: number;
  title: string;
  url: string;
  site?: string;
  date?: string;
  snippet?: string;
}

export interface AgentMessage {
  role: "user" | "agent";
  content: string;
  response?: AgentResponse;
  streamEvidence?: AgentEvidence[];
  /** 联网搜索返回的来源列表，用于展示“已搜索N个网页”与回答内 [n] 角标悬浮卡。 */
  webSources?: AgentWebSource[];
  /** DeepSeek 思考模式的思维链（reasoning_content），仅用于展示，不参与回答内容。 */
  reasoning?: string;
  /** 思考阶段耗时（毫秒），思考结束（正式回答开始）时写入。 */
  reasoningMs?: number;
  action?: {
    type: "process_optimization";
    label: string;
  };
}

export interface AgentConversation {
  sessionId: string;
  title: string;
  updatedAt: string;
  messages: AgentMessage[];
  response: AgentResponse | null;
  pinned?: boolean;
}

export interface AgentEvalSummary {
  total: number;
  passed: number;
  average_score: number;
  results?: Array<{
    case_id: string;
    passed: boolean;
    score: number;
    expected_tools?: string[];
    actual_tools?: string[];
    issues?: string[];
    response_status?: string;
  }>;
  ran_at?: string;
}

/** P0-1: 工具进度 SSE 事件 */
export interface ToolProgressEvent {
  type: 'tool_progress';
  tool_name: string;
  status: 'started' | 'running' | 'completed' | 'error';
  elapsed: number;
  message?: string;
}

/** P0-1: 当前活跃工具状态（供组件展示） */
export interface ActiveTool {
  name: string;
  status: string;
  elapsed: number;
}

