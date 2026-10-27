import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../tauri";
import type { Config, Manifest } from "../types";
import { I } from "./Icons";
import { brainStatus } from "../brainStatus";
import { VoiceSettings } from "./VoiceSettings";

const PROVIDERS: { id: Config["provider"]; name: string; sub: string; local: boolean }[] = [
  { id: "ollama", name: "Ollama", sub: "local · défaut", local: true },
  { id: "lmstudio", name: "LM Studio", sub: "local · API OpenAI", local: true },
  { id: "claude", name: "Claude", sub: "Anthropic · clé API", local: false },
  { id: "gemini", name: "Gemini", sub: "Google · clé API", local: false },
  { id: "openrouter", name: "OpenRouter", sub: "multi-modèles · clé API", local: false },
];

export function SettingsView() {
  const config = useStore((s) => s.config);
  const brain = useStore((s) => s.brain);
  const paths = useStore((s) => s.paths);
  const saveConfig = useStore((s) => s.saveConfig);
  const registry = useStore((s) => s.registry);
  const installed = useStore((s) => s.installed);
  const brainLoading = useStore((s) => s.brainLoading);
  const loadModel = useStore((s) => s.loadModel);
  const refreshBrain = useStore((s) => s.refreshBrain);
  const status = brainStatus(brain, brainLoading);
  const [form, setForm] = useState<Config | null>(config);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [market, setMarket] = useState<{ ok: boolean; msg: string; agents?: Manifest[] } | null>(null);

  useEffect(() => { setForm(config); }, [config]);
  if (!form) return null;

  const set = <K extends keyof Config>(k: K, v: Config[K]) => setForm({ ...form, [k]: v });
  const dirty = JSON.stringify(form) !== JSON.stringify(config);

  const save = async () => {
    setSaving(true);
    await saveConfig(form);
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const testMarket = async () => {
    setMarket({ ok: false, msg: "test en cours…" });
    if (dirty) await saveConfig(form);
    try {
      const agents = await api.marketPreview();
      setMarket({ ok: true, msg: `${agents.length} agent(s) trouvé(s)`, agents });
    } catch (e) {
      setMarket({ ok: false, msg: String(e) });
    }
  };

  return (
    <div className="view settings-view">
      <div className="settings-col">
        <h1>Paramètres</h1>

        <section className="card-section">
          <h2><I.brain size={16} /> Cerveau greffé</h2>
          <p className="muted small">Orchestra et ses agents pensent avec ce modèle. Tout reste local avec Ollama ou LM Studio.</p>
          <div className="provider-grid">
            {PROVIDERS.map((p) => (
              <button key={p.id} className={`provider ${form.provider === p.id ? "active" : ""}`} onClick={() => set("provider", p.id)}>
                <span className="provider-name">{p.name}</span>
                <span className="provider-sub">{p.sub}</span>
                {p.local && <span className="local-badge">local</span>}
              </button>
            ))}
          </div>

          <div className="field-grid">
            <label>
              <span>Modèle <span className="muted">(vide = détection automatique)</span></span>
              <input list="models" value={form.model} onChange={(e) => set("model", e.target.value)} placeholder={brain?.model ? `auto : ${brain.model}` : "auto"} />
              <datalist id="models">{brain?.models.map((m) => <option key={m} value={m} />)}</datalist>
            </label>
            {form.provider === "ollama" && <label><span>URL d'Ollama</span><input value={form.ollama_url} onChange={(e) => set("ollama_url", e.target.value)} /></label>}
            {form.provider === "lmstudio" && <label><span>URL de LM Studio</span><input value={form.lmstudio_url} onChange={(e) => set("lmstudio_url", e.target.value)} /></label>}
            {form.provider === "claude" && <label><span>Clé API Claude</span><input type="password" value={form.claude_api_key} onChange={(e) => set("claude_api_key", e.target.value)} placeholder="sk-ant-…" /></label>}
            {form.provider === "gemini" && <label><span>Clé API Gemini</span><input type="password" value={form.gemini_api_key} onChange={(e) => set("gemini_api_key", e.target.value)} /></label>}
            {form.provider === "openrouter" && <label><span>Clé API OpenRouter</span><input type="password" value={form.openrouter_api_key} onChange={(e) => set("openrouter_api_key", e.target.value)} placeholder="sk-or-…" /></label>}
          </div>

          <div className="detect foot-status" style={{ ["--c" as any]: status.color }}>
            <span className="pill-dot" />
            <span>{status.label}{brain?.model ? ` · ${brain.model}` : ""}</span>
            <span className="muted small">— {brain?.detail}</span>
            {brain && brain.models.length > 0 && <span className="muted small">· {brain.models.length} modèle(s) disponibles</span>}
            {status.action === "load" && <button className="glass-btn sm" onClick={loadModel}><I.power size={11} /> Charger</button>}
            {status.action === "retry" && <button className="glass-btn sm" onClick={refreshBrain}><I.refresh size={11} /> Réessayer</button>}
          </div>
        </section>

        <section className="card-section">
          <h2><I.plug size={16} /> Agent Market</h2>
          <p className="muted small">Quand une tâche dépasse Orchestra, il cherche un agent ici. Format attendu : un JSON <code>[manifeste, …]</code> ou <code>{"{ \"agents\": [...] }"}</code> ; chaque manifeste a <code>id, name, description, capabilities, model, system_prompt, tools</code>. Sans URL, seul le registre local est utilisé.</p>
          <div className="field-grid">
            <label><span>URL de l'Agent Market</span><input value={form.market_url} onChange={(e) => set("market_url", e.target.value)} placeholder="https://… (à fournir)" /></label>
          </div>
          <div className="row">
            <button className="btn" onClick={testMarket} disabled={!form.market_url.trim()}>Tester la connexion</button>
            {market && <span className={`small ${market.ok ? "ok" : "warn"}`}>{market.msg}</span>}
          </div>
          {market?.agents && <div className="chips">{market.agents.map((a) => <span key={a.id} className="chip">{a.name}</span>)}</div>}
          <div className="registry">
            <div className="small"><b>Registre local</b> · {registry.length} agent(s) de démonstration · <b>Installés</b> · {installed.length}</div>
            <div className="chips">{registry.map((a) => <span key={a.id} className="chip ghost" title={a.description}>{a.name}</span>)}</div>
            <button className="link-btn" onClick={() => api.openFolder("agents")}><I.external size={12} /> ouvrir agents/registre-local.json</button>
          </div>
        </section>

        <VoiceSettings form={form} set={set} />

        <section className="card-section">
          <h2><I.memory size={16} /> Mémoire</h2>
          <div className="field-grid">
            <label><span>Ton prénom <span className="muted">(pour la mémoire)</span></span><input value={form.user_name} onChange={(e) => set("user_name", e.target.value)} placeholder="PB" /></label>
            <label><span>Actions outil max. par mission d'agent</span><input type="number" min={1} max={20} value={form.max_agent_steps} onChange={(e) => set("max_agent_steps", Number(e.target.value) || 6)} /></label>
          </div>
          <label className="toggle">
            <input type="checkbox" checked={form.embeddings_enabled} onChange={(e) => set("embeddings_enabled", e.target.checked)} />
            <span>Index vectoriel local (embeddings Ollama)</span>
            <span className="muted small">Améliore le rappel par le sens, en plus des mots communs. Nécessite un modèle d'embeddings dans Ollama.</span>
          </label>
          {form.embeddings_enabled && (
            <div className="field-grid">
              <label><span>Modèle d'embeddings</span><input value={form.embedding_model} onChange={(e) => set("embedding_model", e.target.value)} placeholder="nomic-embed-text" /></label>
            </div>
          )}
        </section>

        <section className="card-section">
          <h2><I.folder size={16} /> Dossiers</h2>
          <p className="muted small">Tout ce qu'Orchestra sait est dans ce dossier, en clair.</p>
          {paths && (
            <ul className="paths">
              <li><code>{paths.root}</code></li>
              <li><code>memoire/orchestra.md</code> — mémoire principale · <code>memoire/agents/*.md</code> — une mémoire par agent · <code>memoire/archives/</code> — conversations</li>
              <li><code>agents/registre-local.json</code> — catalogue local · <code>agents/installes/</code> — manifestes installés</li>
              <li><code>espace-de-travail/</code> — seul dossier où les agents écrivent · <code>journal/</code> — tous les événements</li>
            </ul>
          )}
        </section>

        <div className="save-bar">
          <button className="btn primary" onClick={save} disabled={!dirty || saving}>{saved ? <><I.check size={14} /> Enregistré</> : saving ? "…" : <><I.save size={14} /> Enregistrer et détecter</>}</button>
          {dirty && <span className="muted small">modifications non enregistrées</span>}
        </div>
      </div>
    </div>
  );
}
