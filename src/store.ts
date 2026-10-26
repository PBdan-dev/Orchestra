import { create } from "zustand";
import { api, onEvent } from "./tauri";
import type {
  AgentNode, BrainInfo, ChatMessage, Config, Entry, Manifest, OrchestraEvent, PathsInfo, Phase,
} from "./types";

export type View = "stage" | "memory" | "journal" | "settings";
export type Inspect = { type: "agent"; id: string } | { type: "core" } | null;

let seq = 0;
export const uid = () => `${Date.now().toString(36)}-${(seq++).toString(36)}`;
let bootPromise: Promise<void> | null = null;

/** Énergie partagée avec le canvas (lue à chaque frame, hors React). */
export const live = {
  energy: 0, // 0..1, décroît
  tokens: 0, // tokens reçus récemment
  lastKey: 0,
  pulse(amount = 0.35) {
    live.energy = Math.min(1, live.energy + amount);
  },
};

const pref = {
  get<T>(k: string, d: T): T {
    try {
      const v = localStorage.getItem(`orchestra.${k}`);
      return v === null ? d : (JSON.parse(v) as T);
    } catch { return d; }
  },
  set(k: string, v: unknown) {
    try { localStorage.setItem(`orchestra.${k}`, JSON.stringify(v)); } catch { /* sans stockage */ }
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
  brainLoading: boolean;
  config: Config | null;
  paths: PathsInfo | null;
  registry: Manifest[];
  installed: Manifest[];
  bootDone: boolean;
  memoryFocus: string | null;
  /** Taille du cœur (0.6 à 1.6) */
  coreScale: number;
  /** Hauteur de la zone de saisie, en px */
  composerHeight: number;
  agentsPanel: boolean;
  simulating: boolean;

  setMemoryFocus: (f: string | null) => void;
  setView: (v: View) => void;
  setInspect: (i: Inspect) => void;
  setCoreScale: (s: number) => void;
  setComposerHeight: (h: number) => void;
  setAgentsPanel: (o: boolean) => void;
  boot: () => Promise<void>;
  refreshBrain: () => Promise<void>;
  loadModel: () => Promise<void>;
  refreshAgents: () => Promise<void>;
  saveConfig: (c: Config) => Promise<void>;
  send: (text: string) => Promise<void>;
  listening: () => void;
  handle: (e: OrchestraEvent) => void;
  clearConversation: () => void;
  /** Branche un agent sur la scène à la main (sans mission). */
  plug: (m: Manifest) => void;
  /** Débranche un agent de la scène (ses sous-agents aussi). Rien n'est supprimé sur le disque. */
  unplug: (id: string) => void;
  installAgent: (id: string) => Promise<void>;
  uninstallAgent: (id: string) => Promise<void>;
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
  brainLoading: false,
  config: null,
  paths: null,
  registry: [],
  installed: [],
  bootDone: false,
  memoryFocus: null,
  coreScale: pref.get("coreScale", 1),
  composerHeight: pref.get("composerHeight", 64),
  agentsPanel: false,
  simulating: false,

  setMemoryFocus: (memoryFocus) => set({ memoryFocus }),
  setView: (view) => set({ view }),
  setInspect: (inspect) => set({ inspect }),
  setCoreScale: (s) => {
    const coreScale = Math.max(0.6, Math.min(1.6, Math.round(s * 100) / 100));
    pref.set("coreScale", coreScale);
    set({ coreScale });
  },
  setComposerHeight: (h) => {
    const composerHeight = Math.max(52, Math.min(520, Math.round(h)));
    pref.set("composerHeight", composerHeight);
    set({ composerHeight });
  },
  setAgentsPanel: (agentsPanel) => set({ agentsPanel }),

  boot: () => {
    // idempotent : React StrictMode monte deux fois, on ne s'abonne qu'une fois aux événements
    if (!bootPromise) {
      bootPromise = (async () => {
        const [config, paths] = await Promise.all([api.getConfig(), api.getPaths()]);
        set({ config, paths });
        await onEvent((e) => get().handle(e));
        await Promise.all([get().refreshBrain(), get().refreshAgents()]);
        set({ bootDone: true });
        // revérifie le cerveau régulièrement (Ollama lancé après Orchestra, modèle déchargé…)
        setInterval(() => {
          const s = get();
          if (!s.busy && !s.brainLoading) s.refreshBrain();
        }, 12000);
      })();
    }
    return bootPromise;
  },

  refreshBrain: async () => {
    try {
      const brain = await api.detectBrain();
      set({ brain });
    } catch (e) {
      set({ brain: { provider: get().config?.provider ?? "ollama", model: "", online: false, models: [], detail: String(e), loaded: false, state: "offline" } });
    }
  },

  loadModel: async () => {
    if (get().brainLoading || get().busy) return;
    set({ brainLoading: true, phase: "loading", phaseLabel: `Je charge ${get().brain?.model ?? "le modèle"}` });
    live.pulse(0.6);
    try {
      const brain = await api.loadModel();
      set({ brain });
    } catch (e) {
      set((s) => ({ entries: [...s.entries, { kind: "error", id: uid(), runId: "", at: Date.now(), message: `Chargement du modèle : ${String(e)}` }] }));
      await get().refreshBrain();
    } finally {
      set({ brainLoading: false, phase: "idle", phaseLabel: "" });
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
    if (!t || get().busy || get().simulating) return;
    const history = get().messages.filter((m) => !m.streaming && !m.sim).map((m) => ({ role: m.role, content: m.content }));
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
    if (!get().busy && !get().brainLoading && get().phase !== "listening") set({ phase: "listening", phaseLabel: "J'écoute" });
  },

  clearConversation: () => set({ messages: [], entries: [], agents: {}, inspect: null }),

  plug: (m) => {
    if (get().agents[m.id]) return;
    set((s) => ({
      agents: {
        ...s.agents,
        [m.id]: { manifest: m, status: "attached", source: "branché à la main", reason: "en attente d'une mission", detail: "", toolsUsed: [], attachedAt: Date.now(), runId: "" },
      },
    }));
    live.pulse(0.6);
  },

  unplug: (id) => {
    set((s) => {
      const drop = new Set([id]);
      // retire aussi la descendance (sous-agents)
      let grew = true;
      while (grew) {
        grew = false;
        for (const a of Object.values(s.agents)) {
          if (a.parent && drop.has(a.parent) && !drop.has(a.manifest.id)) { drop.add(a.manifest.id); grew = true; }
        }
      }
      const agents = Object.fromEntries(Object.entries(s.agents).filter(([k]) => !drop.has(k)));
      const inspect = s.inspect?.type === "agent" && drop.has(s.inspect.id) ? null : s.inspect;
      return { agents, inspect };
    });
  },

  installAgent: async (id) => {
    await api.installAgent(id);
    await get().refreshAgents();
  },

  uninstallAgent: async (id) => {
    await api.uninstallAgent(id);
    get().unplug(id);
    await get().refreshAgents();
  },

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
        if (e.run_id !== "sim" && (e.phase === "speaking" || e.phase === "idle")) {
          // le modèle vient forcément d'être chargé
          const b = get().brain;
          if (b && b.state === "not_loaded") set({ brain: { ...b, loaded: true, state: "ready", detail: "modèle chargé, prêt" } });
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
              parent: e.parent,
            },
          },
        }));
        push({ kind: "agent", id: uid(), runId: e.run_id, at: now, agent: e.agent, source: e.source, reason: e.reason, parent: e.parent });
        live.pulse(0.8);
        if (e.run_id !== "sim") get().refreshAgents();
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
        const agent = Object.values(get().agents).find((a) => a.runId === e.run_id && !a.parent && (a.status === "done" || a.status === "error"));
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
