import type { BrainInfo } from "./types";

const PROVIDER: Record<string, string> = {
  ollama: "Ollama", lmstudio: "LM Studio", claude: "Claude", gemini: "Gemini", openrouter: "OpenRouter",
};

export function providerName(p?: string) {
  return (p && PROVIDER[p]) || p || "…";
}

/** État lisible du cerveau : une couleur, un libellé court, et l'action possible. */
export function brainStatus(brain: BrainInfo | null, loading: boolean) {
  const name = providerName(brain?.provider);
  if (loading) return { tone: "loading", color: "#38bdf8", label: `${name} · chargement…`, action: null as null | "load" | "retry" };
  if (!brain) return { tone: "pending", color: "#94a3b8", label: "détection du cerveau…", action: null };
  switch (brain.state) {
    case "ready": return { tone: "ok", color: "#4ade80", label: `${name} · prêt`, action: null };
    case "not_loaded": return { tone: "idle", color: "#fbbf24", label: `${name} · modèle non chargé`, action: "load" as const };
    case "no_model": return { tone: "warn", color: "#fb923c", label: `${name} · aucun modèle installé`, action: "retry" as const };
    case "no_key": return { tone: "warn", color: "#fb923c", label: `${name} · clé API manquante`, action: null };
    default: return { tone: "off", color: "#f87171", label: `${name} · hors ligne`, action: "retry" as const };
  }
}
