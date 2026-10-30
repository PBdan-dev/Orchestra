/**
 * Mode démo navigateur : quand l'interface tourne hors de Tauri (ex. `npm run dev` ouvert
 * dans un navigateur), ce faux backend rejoue un scénario complet pour travailler le design.
 */
import type { BrainInfo, Config, Manifest, MemoryFile, OrchestraEvent, PathsInfo, VoiceStatus } from "./types";

const listeners: ((e: OrchestraEvent) => void)[] = [];
const emit = (e: OrchestraEvent) => listeners.forEach((l) => l(e));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const config: Config = {
  provider: "ollama", model: "", ollama_url: "http://127.0.0.1:11434", lmstudio_url: "http://127.0.0.1:1234",
  claude_api_key: "", gemini_api_key: "", openrouter_api_key: "", market_url: "", embeddings_enabled: false,
  embedding_model: "nomic-embed-text", user_name: "PB", max_agent_steps: 6,
  voice_enabled: false, voice_autostart: true, voice_mode: "mains-libres", voice_ptt_key: "F2", voice_read_chat: false,
  voice_barge_in: true, voice_llm: "qwen3.5:9b", voice_speaker: "Ryan", voice_reference: "", voice_port: 8765,
  voice_stt_model: "Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf", voice_tts_model: "Qwen3-TTS-12Hz-1.7B-CustomVoice",
};
const voiceSt: VoiceStatus = { installed: false, missing: ["démo navigateur : pas de serveur voix"], running: false, mode: "voix", port: 8765, dir: "" };

const redacteur: Manifest = {
  id: "agent-redacteur", name: "Agent Rédacteur", version: "1.0.0", author: "Orchestra (démo)",
  description: "Rédige des textes structurés et les sauvegarde en Markdown.",
  capabilities: ["rédaction", "mail", "note"], model: "inherit",
  system_prompt: "Tu es l'Agent Rédacteur. Tu produis des textes clairs…", tools: ["now", "fetch_url", "write_file", "read_file"],
  color: "#b794f4", icon: "pen",
};
const fichiers: Manifest = { ...redacteur, id: "agent-fichiers", name: "Agent Fichiers", description: "Organise et lit des fichiers.", capabilities: ["fichiers"], tools: ["now", "list_dir", "read_file", "write_file"], color: "#4fd1c5", icon: "folder" };

let mockLoaded = false;
let mockInstalled: Manifest[] = [];

let memory = `# Mémoire d'Orchestra\n\n> Fichier lisible.\n\n## Utilisateur\n- (2026-10-25) Orchestra a été installé pour PB.\n\n## Préférences\n- (2026-10-25) PB aime les interfaces épurées et animées.\n\n## Projets\n\n## Connaissances\n\n## Agents de confiance\n`;

async function stream(run_id: string, text: string, kind: "thought" | "token" = "thought", delay = 18) {
  for (const w of text.split(/(\s+)/)) {
    emit(kind === "thought" ? { type: "thought", run_id, text: w } : { type: "token", run_id, text: w });
    await sleep(delay);
  }
}

async function scenario(run_id: string, text: string) {
  emit({ type: "phase", run_id, phase: "recalling", label: "Je consulte ma mémoire" });
  await sleep(500);
  emit({ type: "recall", run_id, items: [{ file: "orchestra.md", section: "Préférences", text: "PB aime les interfaces épurées et animées.", score: 1.2, reason: "mots communs : interface" }] });
  emit({ type: "phase", run_id, phase: "thinking", label: "J'analyse la demande" });
  await stream(run_id, "L'utilisateur demande une note écrite et sauvegardée : c'est une action concrète, donc un travail à déléguer. Il me faut un agent qui sait rédiger et écrire un fichier.\nJSON: {\"intent\":\"travail\",\"needs_agent\":true,\"need\":\"rédiger une note\",\"mission\":\"Rédiger une note de bienvenue et l'enregistrer dans bienvenue.md\"}");
  emit({ type: "thought", run_id, text: "\n→ Intention : travail · agent nécessaire : oui · besoin : rédiger une note\n" });
  emit({ type: "phase", run_id, phase: "searching_agent", label: "Je cherche l'agent qu'il faut" });
  await sleep(700);
  emit({ type: "thought", run_id, text: "🔎 0 agent déjà installé examiné\n🔎 aucune URL d'Agent Market configurée → repli sur le registre local\n🔎 Registre local : 2 agent(s)\n🔎   Agent Rédacteur (registre local) : score 3.0\n" });
  await sleep(400);
  emit({ type: "agent_attached", run_id, agent: redacteur, source: "registre local", reason: "meilleur score (3.0) pour « rédiger une note »" });
  emit({ type: "phase", run_id, phase: "delegating", label: "Je confie la mission à Agent Rédacteur" });
  emit({ type: "agent_status", run_id, agent_id: redacteur.id, status: "running", detail: "Rédiger une note de bienvenue et l'enregistrer dans bienvenue.md" });
  await sleep(900);
  emit({ type: "phase", run_id, phase: "working", label: "Agent Rédacteur travaille" });
  await stream(run_id, "[Agent Rédacteur] Je vais d'abord dater la note, puis l'écrire dans l'espace de travail.\n");
  emit({ type: "tool_call", run_id, agent_id: redacteur.id, tool: "now", input: {}, output: "dimanche 26 octobre 2026, 00:12" });
  await sleep(800);
  emit({ type: "tool_call", run_id, agent_id: redacteur.id, tool: "write_file", input: { path: "bienvenue.md", content: "# Bienvenue dans Orchestra\n…" }, output: "écrit : espace-de-travail/bienvenue.md (312 caractères)" });
  await sleep(900);
  emit({ type: "agent_status", run_id, agent_id: redacteur.id, status: "done", detail: "Note rédigée et enregistrée dans bienvenue.md" });
  emit({ type: "memorized", run_id, file: "agents/agent-redacteur.md", section: "Missions", line: "- (2026-10-26) Mission : Rédiger une note de bienvenue → Note rédigée et enregistrée dans bienvenue.md · outils : now, write_file" });
  emit({ type: "memorized", run_id, file: "orchestra.md", section: "Agents de confiance", line: "- (2026-10-26) « rédiger une note » → agent-redacteur (Agent Rédacteur) · réussi 1×" });
  emit({ type: "phase", run_id, phase: "speaking", label: "Je restitue le résultat" });
  await stream(run_id, "L'**Agent Rédacteur** a rédigé la note de bienvenue et l'a enregistrée dans `espace-de-travail/bienvenue.md`.\n\nElle présente Orchestra en trois points : ce qu'il fait, comment il délègue, et où se trouve sa mémoire. Tu peux l'ouvrir depuis le dossier de travail.", "token", 28);
  emit({ type: "phase", run_id, phase: "memorizing", label: "Je retiens l'essentiel" });
  await sleep(900);
  emit({ type: "memorized", run_id, file: "orchestra.md", section: "Projets", line: "- (2026-10-26) PB construit Orchestra, un orchestrateur d'agents local et transparent." });
  memory += "- (2026-10-26) PB construit Orchestra, un orchestrateur d'agents local et transparent.\n";
  emit({ type: "done", run_id, answer: "…", intent: "travail" });
  emit({ type: "phase", run_id, phase: "idle", label: "" });
}

/** Démo navigateur : une mission d'agent-fichiers avec une suppression à valider (lot T3). */
let missionDemo: { run_id: string } | null = null;
async function scenarioMission(run_id: string) {
  const mission_id = "2026-10-30_2230_agent-fichiers_demo";
  missionDemo = { run_id };
  emit({ type: "phase", run_id, phase: "delegating", label: "Je prépare la mission de Agent Fichiers" });
  emit({ type: "agent_attached", run_id, agent: fichiers, source: "installé", reason: "supprimer un fichier demandé — meilleur score (4.0)" });
  emit({ type: "mission_debut", run_id, mission_id, agent_id: fichiers.id, projet: "divers", dossier_mission: "", dossier_projet: "" });
  emit({ type: "phase", run_id, phase: "working", label: "Agent Fichiers travaille" });
  await sleep(500);
  emit({ type: "mission_pensee", run_id, mission_id, agent_id: fichiers.id, texte: "La mission demande de supprimer brouillon.md : c'est irréversible, je demande la validation." });
  emit({ type: "mission_action", run_id, mission_id, agent_id: fichiers.id, nature: "observee", action: "action_sensitive_supprimer", detail: "{\"source\":\"brouillon.md\"}", raison: "fichier obsolète" });
  emit({ type: "phase", run_id, phase: "working", label: "J'attends ta validation" });
  emit({ type: "validation_requise", run_id, cle: `${mission_id}#1`, mission_id, agent_id: fichiers.id, action: "action_sensitive_supprimer", raison: "brouillon.md est obsolète (demandé par PB)" });
}

async function finMissionDemo(cle: string, accepte: boolean, portee: string) {
  const run_id = missionDemo?.run_id ?? "demo";
  const mission_id = cle.split("#")[0];
  emit({ type: "validation_reglee", run_id, cle, mission_id, decision: accepte ? "accepte" : "refuse", portee, automatique: false });
  await sleep(400);
  emit({ type: "mission_action", run_id, mission_id, agent_id: fichiers.id, nature: "declaree", action: accepte ? "action_sensitive_supprimer" : "rien", detail: accepte ? "brouillon.md supprimé" : "suppression refusée", raison: "fichier obsolète" });
  emit({ type: "mission_livrables", run_id, mission_id, agent_id: fichiers.id, fichiers: accepte ? [{ chemin: "brouillon.md", existe: false }] : [] });
  emit({ type: "memoire_agent", run_id, mission_id, agent_id: fichiers.id, changements: [{ fichier: "memoire/habitudes.md", etat: "modifié", ajoutees: ["PB valide les suppressions de brouillons"], retirees: [] }] });
  emit({ type: "mission_fin", run_id, mission_id, agent_id: fichiers.id, etat: accepte ? "termine" : "partiel", motif: accepte ? "" : "suppression refusée par PB" });
  emit({ type: "agent_status", run_id, agent_id: fichiers.id, status: "done", detail: accepte ? "brouillon.md supprimé" : "rien supprimé" });
  emit({ type: "token", run_id, text: accepte ? "C'est fait : brouillon.md est supprimé." : "D'accord, je n'ai rien supprimé." });
  emit({ type: "done", run_id, answer: "", intent: "travail" });
  emit({ type: "phase", run_id, phase: "idle", label: "" });
}

export const mockApi = {
  repondreValidation: async (cle: string, accepte: boolean, portee: string) => { setTimeout(() => finMissionDemo(cle, accepte, portee), 100); },
  listerAutorisations: async () => [],
  retirerAutorisation: async () => {},
  reinitialiserMemoireAgent: async () => "",
  listerProjets: async () => ({ actif: "divers", liste: ["divers"] }),
  choisirProjet: async (nom: string) => nom,
  getConfig: async () => config,
  saveConfig: async (c: Config) => { Object.assign(config, c); },
  detectBrain: async (): Promise<BrainInfo> => ({ provider: "ollama", model: "qwen3.5:9b", online: true, models: ["qwen3.5:9b", "qwen3.6:27b"], detail: mockLoaded ? "modèle chargé, prêt" : "modèle non chargé", loaded: mockLoaded, state: mockLoaded ? "ready" : "not_loaded" }),
  loadModel: async (): Promise<BrainInfo> => { await sleep(2500); mockLoaded = true; return mockApi.detectBrain(); },
  installAgent: async (id: string) => { const m = [redacteur, fichiers].find((a) => a.id === id)!; if (!mockInstalled.find((a) => a.id === id)) mockInstalled.push(m); return m; },
  uninstallAgent: async (id: string) => { mockInstalled = mockInstalled.filter((a) => a.id !== id); },
  sendMessage: async (text: string) => { const id = Math.random().toString(36).slice(2, 8); setTimeout(() => (/supprime/i.test(text) ? scenarioMission(id) : scenario(id, text)), 300); return id; },
  listAgents: async () => ({ installed: mockInstalled.slice(), registry: [redacteur, fichiers] }),
  marketPreview: async () => { throw new Error("démo : pas de Market"); },
  memoryTree: async (): Promise<MemoryFile[]> => [{ name: "orchestra.md", rel: "memoire/orchestra.md", kind: "principal", size: memory.length, modified: "26/10/2026 00:12" }],
  readTextFile: async (rel: string) => (rel.endsWith("orchestra.md") ? memory : "# Mémoire de l'agent\n\n## Missions\n- (2026-10-26) Mission : …"),
  writeTextFile: async (_rel: string, content: string) => { memory = content; },
  listJournalDays: async () => ["2026-10-26"],
  readJournal: async () => [] as any[],
  getPaths: async (): Promise<PathsInfo> => ({ root: "C:\\Users\\pb\\Orchestra", memoire: "", agents: "", workspace: "", journal: "" }),
  openFolder: async () => {},
  isBusy: async () => false,
  voiceStatus: async () => voiceSt,
  voiceStart: async (): Promise<VoiceStatus> => { throw new Error("démo navigateur : pas de serveur voix"); },
  voiceStop: async () => voiceSt,
  voiceStartMode: async (): Promise<VoiceStatus> => { throw new Error("démo navigateur : pas de serveur voix"); },
  voiceLog: async () => "",
  voiceModels: async () => ({ stt: ["Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf"], tts: ["Qwen3-TTS-12Hz-1.7B-CustomVoice"], references: [], pocket_voices: [], crisp: false }),
  voiceRestart: async () => voiceSt,
  ollamaSwitch: async () => {},
};

export function mockOnEvent(cb: (e: OrchestraEvent) => void) {
  listeners.push(cb);
  return Promise.resolve(() => { const i = listeners.indexOf(cb); if (i >= 0) listeners.splice(i, 1); });
}
