export type Phase =
  | "idle"
  | "listening"
  | "thinking"
  | "recalling"
  | "searching_agent"
  | "delegating"
  | "working"
  | "speaking"
  | "memorizing"
  | "error";

export interface Manifest {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  capabilities: string[];
  model: string;
  system_prompt: string;
  tools: string[];
  color: string;
  icon: string;
}

export interface Recalled {
  file: string;
  section: string;
  text: string;
  score: number;
  reason: string;
}

export type OrchestraEvent =
  | { type: "phase"; run_id: string; phase: Phase; label: string }
  | { type: "thought"; run_id: string; text: string }
  | { type: "recall"; run_id: string; items: Recalled[] }
  | { type: "agent_attached"; run_id: string; agent: Manifest; source: string; reason: string }
  | { type: "agent_status"; run_id: string; agent_id: string; status: "running" | "done" | "error"; detail: string }
  | { type: "tool_call"; run_id: string; agent_id: string; tool: string; input: unknown; output: string }
  | { type: "token"; run_id: string; text: string }
  | { type: "memorized"; run_id: string; file: string; section: string; line: string }
  | { type: "nothing_to_memorize"; run_id: string }
  | { type: "done"; run_id: string; answer: string; intent: string }
  | { type: "error"; run_id: string; message: string };

export interface Config {
  provider: "ollama" | "lmstudio" | "claude" | "gemini" | "openrouter";
  model: string;
  ollama_url: string;
  lmstudio_url: string;
  claude_api_key: string;
  gemini_api_key: string;
  openrouter_api_key: string;
  market_url: string;
  embeddings_enabled: boolean;
  embedding_model: string;
  user_name: string;
  max_agent_steps: number;
}

export interface BrainInfo {
  provider: string;
  model: string;
  online: boolean;
  models: string[];
  detail: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  intent?: string;
  agent?: string;
  at: number;
}

export interface AgentNode {
  manifest: Manifest;
  status: "attached" | "running" | "done" | "error";
  source: string;
  reason: string;
  detail: string;
  toolsUsed: string[];
  attachedAt: number;
  runId: string;
  lastTool?: { tool: string; at: number };
}

export type Entry =
  | { kind: "phase"; id: string; runId: string; at: number; phase: Phase; label: string }
  | { kind: "thought"; id: string; runId: string; at: number; text: string }
  | { kind: "recall"; id: string; runId: string; at: number; items: Recalled[] }
  | { kind: "agent"; id: string; runId: string; at: number; agent: Manifest; source: string; reason: string }
  | { kind: "agent_status"; id: string; runId: string; at: number; agentId: string; status: string; detail: string }
  | { kind: "tool"; id: string; runId: string; at: number; agentId: string; tool: string; input: unknown; output: string }
  | { kind: "memorized"; id: string; runId: string; at: number; file: string; section: string; line: string }
  | { kind: "nothing"; id: string; runId: string; at: number }
  | { kind: "error"; id: string; runId: string; at: number; message: string }
  | { kind: "user"; id: string; runId: string; at: number; text: string };

export interface MemoryFile {
  name: string;
  rel: string;
  kind: "principal" | "agent" | "archive";
  size: number;
  modified: string;
}

export interface PathsInfo {
  root: string;
  memoire: string;
  agents: string;
  workspace: string;
  journal: string;
}

export const PHASE_LABEL: Record<Phase, string> = {
  idle: "En veille",
  listening: "J'écoute",
  thinking: "Je réfléchis",
  recalling: "Je me souviens",
  searching_agent: "Je cherche un agent",
  delegating: "Je délègue",
  working: "Un agent travaille",
  speaking: "Je réponds",
  memorizing: "Je retiens",
  error: "Erreur",
};

export const PHASE_COLOR: Record<Phase, string> = {
  idle: "#7c8cf8",
  listening: "#5eead4",
  thinking: "#a78bfa",
  recalling: "#60a5fa",
  searching_agent: "#f0abfc",
  delegating: "#fbbf24",
  working: "#fb923c",
  speaking: "#34d399",
  memorizing: "#4ade80",
  error: "#f87171",
};
