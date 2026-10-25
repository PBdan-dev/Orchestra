import { create } from "zustand";
import { api, onEvent } from "./tauri";
import type {
  AgentNode, BrainInfo, ChatMessage, Config, Entry, Manifest, OrchestraEvent, PathsInfo, Phase,
} from "./types";

export type View = "stage" | "memory" | "journal" | "settings";
export type Inspect = { type: "agent"; id: string } | { type: "core" } | null;

let seq = 0;
const uid = () => `${Date.now().toString(36)}-${(seq++).toString(36)}`;

/** Énergie partagée avec le canvas (lue à chaque frame, hors React). */
export const live = {
  energy: 0, // 0..1, décroît
  tokens: 0, // tokens reçus récemment
  lastKey: 0,
  pulse(amount = 0.35) {
    live.energy = Math.min(1, live.energy + amount);
  },
};

interface State {
  view: View;
  inspect: Inspect;
  phase: Phase;
  phaseLabel: string;
  runId: string | null;
  busy: boolean;
  messages: ChatMessage[];
  entries: Entry[];
  agents: Record<string, AgentNode>;
  brain: BrainInfo | null;
  config: Config | null;
  paths: PathsInfo | null;
  registry: Manifest[];
  installed: Manifest[];
  bootDone: boolean;
  memoryFocus: string | null;

  setMemoryFocus: (f: string | null) => void;
  setView: (v: View) => void;
  setInspect: (i: Inspect) => void;
  boot: () => Promise<void>;
  refreshBrain: () => Promise<void>;
  refreshAgents: () => Promise<void>;
  saveConfig: (c: Config) => Promise<void>;
  send: (text: string) => Promise<void>;
  listening: () => void;
  handle: (e: OrchestraEvent) => void;
  clearConversation: () => void;
}

export const useStore = create<State>((set, get) => ({
  view: "stage",
  inspect: null,
  phase: "idle",
  phaseLabel: "",
  runId: null,
  busy: false,
  messages: [],
  entries: [],
  agents: {},
  brain: null,
  config: null,
  paths: null,
  registry: [],
  installed: [],
  bootDone: false,
  memoryFocus: null,

  setMemoryFocus: (memoryFocus) => set({ memoryFocus }),
  setView: (view) => set({ view }),
  setInspect: (inspect) => set({ inspect }),

  boot: async () => {
    const [config, paths] = await Promise.all([api.getConfig(), api.getPaths()]);
    set({ config, paths });
    await onEvent((e) => get().handle(e));
    await Promise.all([get().refreshBrain(), get().refreshAgents()]);
    set({ bootDone: true });
  },

  refreshBrain: async () => {
    try {
      const brain = await api.detectBrain();
      set({ brain });
    } catch (e) {
      set({ brain: { provider: get().config?.provider ?? "ollama", model: "", online: false, models: [], detail: String(e) } });
    }
  },

  refreshAgents: async () => {
    const { installed, registry } = await api.listAgents();
    set({ installed, registry });
  },

  saveConfig: async (config) => {
    await api.saveConfig(config);
    set({ config });
    await get().refreshBrain();
  },

  send: async (text) => {
    const t = text.trim();
    if (!t || get().busy) return;
    const history = get().messages.filter((m) => !m.streaming).map((m) => ({ role: m.role, content: m.content }));
    const userMsg: ChatMessage = { id: uid(), role: "user", content: t, at: Date.now() };
    const draft: ChatMessage = { id: uid(), role: "assistant", content: "", streaming: true, at: Date.now() };
    set((s) => ({
      busy: true,
      phase: "thinking",
      phaseLabel: "Je réfléchis",
      messages: [...s.messages, userMsg, draft],
      entries: [...s.entries, { kind: "user", id: uid(), runId: "", at: Date.now(), text: t }],
    }));
    live.pulse(0.6);
    try {
      const runId = await api.sendMessage(t, history);
      set({ runId });
    } catch (e) {
      set((s) => ({
        busy: false,
        phase: "error",
        phaseLabel: "Erreur",
        entries: [...s.entries, { kind: "error", id: uid(), runId: "", at: Date.now(), message: String(e) }],
        messages: s.messages.map((m) => (m.id === draft.id ? { ...m, streaming: false, content: `⚠ ${String(e)}` } : m)),
      }));
      setTimeout(() => set({ phase: "idle" }), 2500);
    }
  },

  listening: () => {
    live.lastKey = Date.now();
    live.pulse(0.18);
    if (!get().busy && get().phase !== "listening") set({ phase: "listening", phaseLabel: "J'écoute" });
  },

  clearConversation: () => set({ messages: [], entries: [], agents: {}, inspect: null }),

  handle: (e) => {
    const now = Date.now();
    const push = (entry: Entry) => set((s) => ({ entries: [...s.entries, entry] }));
    switch (e.type) {
      case "phase": {
        set({ phase: e.phase, phaseLabel: e.label });
        if (e.phase !== "idle") {
          push({ kind: "phase", id: uid(), runId: e.run_id, at: now, phase: e.phase, label: e.label });
          live.pulse(0.3);
        }
        break;
      }
      case "thought": {
        live.pulse(0.06);
        set((s) => {
          const last = s.entries[s.entries.length - 1];
          if (last && last.kind === "thought" && last.runId === e.run_id) {
            const updated = { ...last, text: last.text + e.text };
            return { entries: [...s.entries.slice(0, -1), updated] };
          }
          return { entries: [...s.entries, { kind: "thought", id: uid(), runId: e.run_id, at: now, text: e.text }] };
        });
        break;
      }
      case "recall":
        push({ kind: "recall", id: uid(), runId: e.run_id, at: now, items: e.items });
        live.pulse(0.4);
        break;
      case "agent_attached": {
        set((s) => ({
          agents: {
            ...s.agents,
            [e.agent.id]: {
              manifest: e.agent,
              status: "attached",
              source: e.source,
              reason: e.reason,
              detail: "",
              toolsUsed: s.agents[e.agent.id]?.toolsUsed ?? [],
              attachedAt: s.agents[e.agent.id]?.attachedAt ?? now,
              runId: e.run_id,
            },
          },
        }));
        push({ kind: "agent", id: uid(), runId: e.run_id, at: now, agent: e.agent, source: e.source, reason: e.reason });
        live.pulse(0.8);
        get().refreshAgents();
        break;
      }
      case "agent_status": {
        set((s) => {
          const a = s.agents[e.agent_id];
          if (!a) return {};
          return { agents: { ...s.agents, [e.agent_id]: { ...a, status: e.status, detail: e.detail } } };
        });
        push({ kind: "agent_status", id: uid(), runId: e.run_id, at: now, agentId: e.agent_id, status: e.status, detail: e.detail });
        break;
      }
      case "tool_call": {
        set((s) => {
          const a = s.agents[e.agent_id];
          if (!a) return {};
          const toolsUsed = a.toolsUsed.includes(e.tool) ? a.toolsUsed : [...a.toolsUsed, e.tool];
          return { agents: { ...s.agents, [e.agent_id]: { ...a, toolsUsed, lastTool: { tool: e.tool, at: now } } } };
        });
        push({ kind: "tool", id: uid(), runId: e.run_id, at: now, agentId: e.agent_id, tool: e.tool, input: e.input, output: e.output });
        live.pulse(0.5);
        break;
      }
      case "token": {
        live.tokens += 1;
        live.pulse(0.08);
        set((s) => {
          const idx = s.messages.findIndex((m) => m.streaming);
          if (idx < 0) return {};
          const msgs = s.messages.slice();
          msgs[idx] = { ...msgs[idx], content: msgs[idx].content + e.text };
          return { messages: msgs };
        });
        break;
      }
      case "memorized":
        push({ kind: "memorized", id: uid(), runId: e.run_id, at: now, file: e.file, section: e.section, line: e.line });
        live.pulse(0.4);
        break;
      case "nothing_to_memorize":
        push({ kind: "nothing", id: uid(), runId: e.run_id, at: now });
        break;
      case "done": {
        const agent = Object.values(get().agents).find((a) => a.runId === e.run_id && (a.status === "done" || a.status === "error"));
        set((s) => ({
          busy: false,
          runId: null,
          messages: s.messages.map((m) =>
            m.streaming ? { ...m, streaming: false, content: m.content || e.answer, intent: e.intent, agent: agent?.manifest.name } : m,
          ),
        }));
        break;
      }
      case "error": {
        push({ kind: "error", id: uid(), runId: e.run_id, at: now, message: e.message });
        set((s) => ({
          busy: false,
          runId: null,
          phase: "error",
          phaseLabel: "Erreur",
          messages: s.messages.map((m) => (m.streaming ? { ...m, streaming: false, content: m.content || `⚠ ${e.message}` } : m)),
        }));
        setTimeout(() => { if (get().phase === "error") set({ phase: "idle" }); }, 3000);
        break;
      }
    }
  },
}));
