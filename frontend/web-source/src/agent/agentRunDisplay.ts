const INTERNAL_BLOCK_PATTERNS = [
  /<memory_compiler\b[\s\S]*?<\/memory_compiler>/gi,
  /<agent_internal\b[\s\S]*?<\/agent_internal>/gi,
  /<context_metadata\b[\s\S]*?<\/context_metadata>/gi,
  /<tool_payload\b[\s\S]*?<\/tool_payload>/gi,
  /```(?:agent-internal|memory-compiler|context-metadata|tool-payload|internal)[\s\S]*?```/gi,
  /<!--\s*agent-internal[\s\S]*?-->/gi,
];

export function stripInternalAgentBlocks(text: string): string {
  let cleaned = text;
  for (const pattern of INTERNAL_BLOCK_PATTERNS) {
    cleaned = cleaned.replace(pattern, "\n");
  }
  return cleaned.replace(/[ \t]+\n/g, "\n").replace(/\n{2,}/g, "\n").trim();
}
