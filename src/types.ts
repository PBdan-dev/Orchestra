export type Phase =
  | "idle"
  | "loading"
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
  /** orchestra_certified | livrable_seul | non_certifie */
  certification?: string;
  /** Agent installé au format devenu incompatible : utilisable, avec cet avertissement (D37). */
  avertissement?: string;
}

/** Identifiant déclaré par un agent (D34), rangé dans le coffre de Windows. */
export interface Identifiant { nom: string; type: string; raison: string; present: boolean }
export interface Installation { manifest: Manifest; ancienne_version: string | null; identifiants: Identifiant[]; certification: string }
export interface EntreeCatalogue { manifest: Manifest; installable: boolean; motif: string; installe: string | null }
export interface Catalogue { entrees: EntreeCatalogue[]; ignores: string[]; source: string }

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
  | { type: "agent_attached"; run_id: string; agent: Manifest; source: string; reason: string; parent?: string }
  | { type: "agent_status"; run_id: string; agent_id: string; status: "running" | "done" | "error"; detail: string }
  | { type: "tool_call"; run_id: string; agent_id: string; tool: string; input: unknown; output: string }
  | { type: "token"; run_id: string; text: string }
  | { type: "memorized"; run_id: string; file: string; section: string; line: string }
  | { type: "nothing_to_memorize"; run_id: string }
  | { type: "done"; run_id: string; answer: string; intent: string; message_historique?: string }
  | { type: "coupe"; run_id: string; quoi: string; detail: string }
  | { type: "avertissement"; run_id: string; message: string }
  | { type: "mission_verification"; run_id: string; mission_id: string; agent_id: string; ecarts: string[]; controles: string[]; statut: string; total_ecarts: number; confiance_retiree: boolean }
  | { type: "integrite"; contexte: string; intactes: number; non_chainees: number; ruptures: Rupture[]; deja_signalees: number }
  | { type: "limite_atteinte"; run_id: string; mission_id: string; agent_id: string; texte: string }
  | { type: "mission_ordre"; mission_id: string; ordre: string }
  | { type: "agent_installe"; agent_id: string; version: string; ancienne_version: string | null; certification: string; identifiants_manquants: string[] }
  | { type: "mission_debut"; run_id: string; mission_id: string; agent_id: string; projet: string; dossier_mission: string; dossier_projet: string }
  | { type: "mission_pensee"; run_id: string; mission_id: string; agent_id: string; texte: string }
  | { type: "mission_action"; run_id: string; mission_id: string; agent_id: string; nature: "observee" | "declaree"; action: string; detail: string; raison: string }
  | { type: "mission_livrables"; run_id: string; mission_id: string; agent_id: string; fichiers: Livrable[] }
  | { type: "memoire_agent"; run_id: string; mission_id: string; agent_id: string; changements: ChangementMemoire[] }
  | { type: "mission_fin"; run_id: string; mission_id: string; agent_id: string; etat: string; motif: string }
  | { type: "validation_requise"; run_id: string; cle: string; mission_id: string; agent_id: string; action: string; raison: string }
  | { type: "validation_reglee"; run_id: string; cle: string; mission_id: string; decision: "accepte" | "refuse"; portee: string; automatique: boolean }
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
  mode_developpement: boolean;
  /** projet où vont les missions et les livrables (D36) */
  projet_actif?: string;
  embeddings_enabled: boolean;
  embedding_model: string;
  user_name: string;
  max_agent_steps: number;
  voice_enabled: boolean;
  voice_autostart: boolean;
  voice_mode: "mains-libres" | "appuyer";
  voice_ptt_key: string;
  voice_read_chat: boolean;
  voice_barge_in: boolean;
  voice_llm: string;
  voice_speaker: string;
  voice_reference: string;
  voice_port: number;
  voice_stt_model: string;
  voice_tts_model: string;
}

export interface VoiceModels {
  stt: string[];
  tts: string[];
  references: string[];
  pocket_voices: string[];
  crisp: boolean;
}

export interface VoiceStatus {
  installed: boolean;
  missing: string[];
  running: boolean;
  mode: string;
  port: number;
  dir: string;
}

export interface VoiceLatency {
  total_ms: number;
  vad_ms: number;
  stt_ms: number | null;
  llm_ms: number | null;
  tts_ms: number | null;
  at?: string;
}

export interface BrainInfo {
  provider: string;
  model: string;
  online: boolean;
  models: string[];
  detail: string;
  loaded: boolean;
  state: "offline" | "not_loaded" | "ready" | "no_model" | "no_key";
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  /** message issu d'une simulation : jamais renvoyé au modèle */
  sim?: boolean;
  /** message dit à voix haute (mode vocal) */
  voice?: boolean;
  intent?: string;
  agent?: string;
  /** note d'Orchestra (file d'attente…) : affichée, jamais renvoyée au modèle */
  note?: boolean;
  /** version du message envoyée au cerveau (date et souvenirs compris), renvoyée telle quelle dans l'historique */
  contexte?: string;
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
  /** id de l'agent parent quand c'est un sous-agent */
  parent?: string;
}

export interface Livrable { chemin: string; existe: boolean }
export interface ChangementMemoire { fichier: string; etat: string; ajoutees: string[]; retirees: string[] }
export interface ActionMission { nature: "observee" | "declaree"; action: string; detail: string; raison: string }
export interface ValidationMission { cle: string; action: string; raison: string; etat: "attente" | "accepte" | "refuse"; portee?: string; automatique?: boolean }
export interface MissionVue {
  missionId: string;
  agentId: string;
  projet: string;
  dossierMission: string;
  dossierProjet: string;
  pensees: string[];
  actions: ActionMission[];
  livrables: Livrable[] | null;
  memoire: ChangementMemoire[];
  validations: ValidationMission[];
  etat?: string;
  motif?: string;
  /** Vérification de la mission (lot T5). */
  verification?: VerificationMission;
  /** Pilotage (lot T7) : en pause, arrêt demandé, limite atteinte. */
  enPause?: boolean;
  arretDemande?: boolean;
  limite?: string;
}
/** Vue « Missions » (lot T7). */
export interface MissionResume { id: string; date: string; projet: string; agent_id: string; but: string; etat: string; ecarts: number | null; reprenable: boolean }
export interface MemoireGraphique { totale: number; utilisee: number; detail: string }
export interface VerificationMission { ecarts: string[]; controles: string[]; statut: string; totalEcarts: number; confianceRetiree: boolean }
/** Trace dont la chaîne d'empreintes est cassée (lot T6). */
export interface Rupture { fichier: string; ligne: number; motif: string; agent_id: string | null; mission_id: string | null; preuve: string }
export interface BilanIntegrite { intactes: number; non_chainees: number; ruptures: Rupture[]; deja_signalees: number }
/** Confiance dans un agent (lot T5, agents/confiance.json). */
export interface EtatConfiance { statut: "normal" | "retire" | "quarantaine"; ecarts: number; historique: unknown[] }
/** Autorisation d'actions irréversibles (D28) */
export interface Autorisation { agent_id: string; action: string; portee: "type_session" | "tout_session" | "permanent" }
export interface Projets { actif: string; liste: string[] }

export type Entry =
  | { kind: "phase"; id: string; runId: string; at: number; phase: Phase; label: string }
  | { kind: "thought"; id: string; runId: string; at: number; text: string }
  | { kind: "recall"; id: string; runId: string; at: number; items: Recalled[] }
  | { kind: "agent"; id: string; runId: string; at: number; agent: Manifest; source: string; reason: string; parent?: string }
  | { kind: "agent_status"; id: string; runId: string; at: number; agentId: string; status: string; detail: string }
  | { kind: "tool"; id: string; runId: string; at: number; agentId: string; tool: string; input: unknown; output: string }
  | { kind: "memorized"; id: string; runId: string; at: number; file: string; section: string; line: string }
  | { kind: "nothing"; id: string; runId: string; at: number }
  | { kind: "error"; id: string; runId: string; at: number; message: string }
  | { kind: "avertissement"; id: string; runId: string; at: number; message: string }
  | { kind: "user"; id: string; runId: string; at: number; text: string }
  | ({ kind: "mission"; id: string; runId: string; at: number } & MissionVue);

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
  loading: "Je charge le modèle",
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
  idle: "#7dd3fc",
  loading: "#38bdf8",
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
