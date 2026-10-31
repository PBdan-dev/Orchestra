import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Autorisation, BilanIntegrite, MemoireGraphique, MissionResume, BrainInfo, Catalogue, EtatConfiance, Identifiant, Installation, Config, Manifest, MemoryFile, OrchestraEvent, PathsInfo, Projets, VoiceModels, VoiceStatus } from "./types";
import { mockApi, mockOnEvent } from "./mock";

/** Vrai quand l'interface tourne dans la fenêtre Tauri ; sinon on est en démo navigateur. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const real = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  detectBrain: () => invoke<BrainInfo>("detect_brain"),
  loadModel: () => invoke<BrainInfo>("load_model"),
  installAgent: (id: string) => invoke<Installation>("install_agent", { id }),
  installAgentZip: (chemin: string) => invoke<Installation>("install_agent_zip", { chemin }),
  catalogue: () => invoke<Catalogue>("catalogue"),
  compacter: (history: { role: string; content: string }[]) => invoke<boolean>("compacter", { history }),
  annulerCompaction: () => invoke<boolean>("annuler_compaction"),
  etatSession: () => invoke<{ couvre: number; resume: string | null }>("etat_session"),
  commanderMission: (missionId: string, ordre: string) => invoke<void>("commander_mission", { missionId, ordre }),
  reprendreMission: (missionId: string) => invoke<string>("reprendre_mission", { missionId }),
  listerMissions: () => invoke<MissionResume[]>("lister_missions"),
  memoireGraphique: () => invoke<MemoireGraphique>("memoire_graphique"),
  verifierIntegrite: (mission: string | null) => invoke<BilanIntegrite>("verifier_integrite", { mission }),
  confianceAgents: () => invoke<Record<string, EtatConfiance>>("confiance_agents"),
  leverQuarantaine: (id: string) => invoke<void>("lever_quarantaine", { id }),
  retablirConfiance: (id: string) => invoke<void>("retablir_confiance", { id }),
  identifiantsAgent: (id: string) => invoke<Identifiant[]>("identifiants_agent", { id }),
  enregistrerIdentifiant: (id: string, nom: string, valeur: string) => invoke<void>("enregistrer_identifiant", { id, nom, valeur }),
  uninstallAgent: (id: string) => invoke<void>("uninstall_agent", { id }),
  sendMessage: (text: string, history: { role: string; content: string }[]) =>
    invoke<string>("send_message", { text, history }),
  listAgents: () => invoke<{ installed: Manifest[]; registry: Manifest[] }>("list_agents"),
  marketPreview: () => invoke<Manifest[]>("market_preview"),
  memoryTree: () => invoke<MemoryFile[]>("memory_tree"),
  readTextFile: (rel: string) => invoke<string>("read_text_file", { rel }),
  writeTextFile: (rel: string, content: string) => invoke<void>("write_text_file", { rel, content }),
  listJournalDays: () => invoke<string[]>("list_journal_days"),
  readJournal: (day: string) => invoke<any[]>("read_journal", { day }),
  getPaths: () => invoke<PathsInfo>("get_paths"),
  openFolder: (which: string) => invoke<void>("open_folder", { which }),
  isBusy: () => invoke<boolean>("is_busy"),
  voiceStatus: () => invoke<VoiceStatus>("voice_status"),
  voiceStart: () => invoke<VoiceStatus>("voice_start"),
  voiceStop: () => invoke<VoiceStatus>("voice_stop"),
  voiceStartMode: (mode: "voix" | "dictee") => invoke<VoiceStatus>("voice_start_mode", { mode }),
  voiceLog: () => invoke<string>("voice_log"),
  voiceModels: () => invoke<VoiceModels>("voice_models"),
  voiceRestart: () => invoke<VoiceStatus>("voice_restart"),
  ollamaSwitch: (load: string | null, unload: string | null) => invoke<void>("ollama_switch", { load, unload }),
  repondreValidation: (cle: string, accepte: boolean, portee: string) => invoke<void>("repondre_validation", { cle, accepte, portee }),
  listerAutorisations: () => invoke<Autorisation[]>("lister_autorisations"),
  retirerAutorisation: (autorisation: Autorisation) => invoke<void>("retirer_autorisation", { autorisation }),
  reinitialiserMemoireAgent: (id: string) => invoke<string>("reinitialiser_memoire_agent", { id }),
  listerProjets: () => invoke<Projets>("lister_projets"),
  choisirProjet: (nom: string) => invoke<string>("choisir_projet", { nom }),
};

export const api: typeof real = isTauri ? real : (mockApi as unknown as typeof real);

export function onEvent(cb: (e: OrchestraEvent) => void) {
  if (!isTauri) return mockOnEvent(cb);
  return listen<OrchestraEvent>("orchestra://event", (ev) => cb(ev.payload));
}
