import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BrainInfo, Config, Manifest, MemoryFile, OrchestraEvent, PathsInfo } from "./types";
import { mockApi, mockOnEvent } from "./mock";

/** Vrai quand l'interface tourne dans la fenêtre Tauri ; sinon on est en démo navigateur. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const real = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  detectBrain: () => invoke<BrainInfo>("detect_brain"),
  loadModel: () => invoke<BrainInfo>("load_model"),
  installAgent: (id: string) => invoke<Manifest>("install_agent", { id }),
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
};

export const api: typeof real = isTauri ? real : (mockApi as unknown as typeof real);

export function onEvent(cb: (e: OrchestraEvent) => void) {
  if (!isTauri) return mockOnEvent(cb);
  return listen<OrchestraEvent>("orchestra://event", (ev) => cb(ev.payload));
}
