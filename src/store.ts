import { create } from "zustand";
import { api, onEvent } from "./tauri";
import { VoiceClient, type VoiceEvent } from "./voice";
import type {
  AgentNode, BrainInfo, ChatMessage, Config, Entry, Manifest, OrchestraEvent, PathsInfo, Phase, VoiceLatency, VoiceStatus,
} from "./types";

export type View = "stage" | "memory" | "journal" | "settings";
export type Inspect = { type: "agent"; id: string } | { type: "core" } | null;

let seq = 0;
export const uid = () => `${Date.now().toString(36)}-${(seq++).toString(36)}`;
let bootPromise: Promise<void> | null = null;
let voiceDraft: string | null = null;
/** vrai quand le run en cours a été lancé par la voix : son résultat sera lu à voix haute */
let voiceRun = false;
export let voice: VoiceClient;
const historyOf = (msgs: ChatMessage[]) => msgs.filter((m) => !m.streaming && !m.sim).map((m) => ({ role: m.role, content: m.content }));

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
  /** mode vocal */
  voiceStatus: VoiceStatus | null;
  voiceConn: "off" | "connecting" | "on" | "error";
  /** session vocale ouverte (micro actif) */
  voiceOn: boolean;
  /** la personne parle (détecté par le serveur) */
  voiceUser: boolean;
  /** appuyer-pour-parler enfoncé */
  voiceHeld: boolean;
  voiceInterim: string;
  voiceLatency: VoiceLatency | null;
  voiceError: string;

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
  refreshVoice: () => Promise<void>;
  /** ouvre ou ferme la session vocale (démarre le serveur si besoin) */
  toggleVoice: () => Promise<void>;
  /** appuyer-pour-parler */
  holdVoice: (down: boolean) => void;
  stopSpeaking: () => void;
  onVoice: (e: VoiceEvent) => void;
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
  voiceStatus: null,
  voiceConn: "off",
  voiceOn: false,
  voiceUser: false,
  voiceHeld: false,
  voiceInterim: "",
  voiceLatency: null,
  voiceError: "",

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
        voice = new VoiceClient((e) => get().onVoice(e));
        await Promise.all([get().refreshBrain(), get().refreshAgents(), get().refreshVoice()]);
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

  refreshVoice: async () => {
    try { set({ voiceStatus: await api.voiceStatus() }); } catch { /* sans module voix */ }
  },

  toggleVoice: async () => {
    const s = get();
    if (s.voiceOn) {
      voice.disconnect();
      set({ voiceOn: false, voiceUser: false, voiceHeld: false, voiceInterim: "", voiceConn: "off" });
      if ((s.phase === "listening" || s.phase === "speaking") && !s.busy) set({ phase: "idle", phaseLabel: "" });
      return;
    }
    set({ voiceError: "" });
    try {
      let st = await api.voiceStatus();
      if (!st.installed) throw new Error(`module voix incomplet : ${st.missing.join(", ")}`);
      if (!st.running) st = await api.voiceStart();
      set({ voiceStatus: st });
      const cfg = get().config!;
      voice.muteWhileSpeaking = !cfg.voice_barge_in;
      voice.gate = cfg.voice_mode !== "appuyer";
      await voice.startMic();
      await voice.connect(st.port);
      set({ voiceOn: true });
    } catch (e) {
      set({ voiceError: String(e).replace(/^Error: /, ""), voiceOn: false });
    }
  },

  holdVoice: (down) => {
    const s = get();
    if (!s.voiceOn || s.config?.voice_mode !== "appuyer" || s.voiceHeld === down) return;
    if (down && voice.playing) voice.stopSpeaking();
    voice.gate = down;
    set({ voiceHeld: down });
    if (down && !s.busy) set({ phase: "listening", phaseLabel: "Je t'écoute" });
  },

  stopSpeaking: () => voice?.stopSpeaking(),

  onVoice: (e) => {
    const s = get();
    switch (e.type) {
      case "conn":
        set({ voiceConn: e.state });
        if (e.state === "on") voice.send({ type: "context", messages: historyOf(s.messages) });
        break;
      case "user_start":
        voice.send({ type: "context", messages: historyOf(s.messages) });
        set({ voiceUser: true, voiceInterim: "" });
        if (!s.busy) set({ phase: "listening", phaseLabel: "Je t'écoute" });
        live.pulse(0.3);
        break;
      case "user_stop":
        set({ voiceUser: false });
        if (!s.busy) set({ phase: "thinking", phaseLabel: "Je réfléchis" });
        break;
      case "user_interim":
        set({ voiceInterim: e.text });
        break;
      case "user_final":
        set((st) => ({
          voiceInterim: "",
          messages: [...st.messages, { id: uid(), role: "user", content: e.text, voice: true, at: Date.now() }],
          entries: [...st.entries, { kind: "user", id: uid(), runId: "voix", at: Date.now(), text: `🎙 ${e.text}` }],
        }));
        break;
      case "bot_start": {
        const id = uid();
        voiceDraft = id;
        set((st) => ({ messages: [...st.messages, { id, role: "assistant", content: "", streaming: true, voice: true, at: Date.now() }] }));
        break;
      }
      case "bot_text":
        live.tokens += 1;
        set((st) => ({ messages: st.messages.map((m) => (m.id === voiceDraft ? { ...m, content: m.content + e.text } : m)) }));
        break;
      case "bot_end":
        set((st) => ({
          messages: st.messages
            .map((m) => (m.id === voiceDraft ? { ...m, streaming: false } : m))
            .filter((m) => !(m.id === voiceDraft && !m.content.trim())),
        }));
        voiceDraft = null;
        break;
      case "delegate":
        // la voix confie un travail à la boucle centrale : même chemin qu'un message tapé
        if (e.text && !s.busy) {
          voiceRun = true;
          setTimeout(() => get().send(e.text), 0);
        }
        break;
      case "playback":
        if (e.playing) set({ phase: "speaking", phaseLabel: "Je parle" });
        else if (!get().busy) set({ phase: "idle", phaseLabel: "" });
        break;
      case "latency":
        set({ voiceLatency: { total_ms: e.total_ms, vad_ms: e.vad_ms, stt_ms: e.stt_ms, llm_ms: e.llm_ms, tts_ms: e.tts_ms, at: e.at as string } });
        break;
      default:
        break;
    }
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
          const idx = s.messages.findIndex((m) => m.streaming && !m.voice);
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
            m.streaming && !m.voice ? { ...m, streaming: false, content: m.content || e.answer, intent: e.intent, agent: agent?.manifest.name } : m,
          ),
        }));
        // réponse lue à voix haute : toujours si la demande venait de la voix, sinon selon le réglage
        if (e.run_id !== "sim" && voice?.connected && (voiceRun || get().config?.voice_read_chat)) {
          const last = [...get().messages].reverse().find((m) => m.role === "assistant" && !m.voice);
          if (last?.content) voice.say(last.content);
        }
        voiceRun = false;
        break;
      }
      case "error": {
        push({ kind: "error", id: uid(), runId: e.run_id, at: now, message: e.message });
        set((s) => ({
          busy: false,
          runId: null,
          phase: "error",
          phaseLabel: "Erreur",
          messages: s.messages.map((m) => (m.streaming && !m.voice ? { ...m, streaming: false, content: m.content || `⚠ ${e.message}` } : m)),
        }));
        if (voiceRun && voice?.connected) voice.say("La tâche a échoué. Le détail est affiché à l'écran.");
        voiceRun = false;
        setTimeout(() => { if (get().phase === "error") set({ phase: "idle" }); }, 3000);
        break;
      }
    }
  },
}));
