import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../tauri";
import type { Config } from "../types";
import { I } from "./Icons";

const SPEAKERS = ["Ryan", "Aiden", "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ono_Anna", "Sohee"];

/** Réglages du mode vocal (module optionnel : désactivé, il ne change rien au reste). */
export function VoiceSettings({ form, set }: { form: Config; set: <K extends keyof Config>(k: K, v: Config[K]) => void }) {
  const status = useStore((s) => s.voiceStatus);
  const refresh = useStore((s) => s.refreshVoice);
  const latency = useStore((s) => s.voiceLatency);
  const [log, setLog] = useState<string>("");
  const [busy, setBusy] = useState(false);

  useEffect(() => { refresh(); }, []);

  const startStop = async () => {
    setBusy(true);
    try {
      if (status?.running) await api.voiceStop(); else await api.voiceStart();
    } catch (e) {
      setLog(String(e));
    }
    await refresh();
    setBusy(false);
  };

  return (
    <section className="card-section">
      <h2><I.mic size={16} /> Voix</h2>
      <p className="muted small">
        Les modèles, la voix et les consignes se choisissent dans le panneau du cœur (clic sur le cœur), et le passage
        du chat à la voix avec le switch qui s'y trouve. Ici, seulement l'état du serveur voix et la façon de parler.
      </p>

      {(
        <>
          <div className="detect foot-status" style={{ ["--c" as any]: status?.installed ? (status.running ? "#34d399" : "#7dd3fc") : "#fbbf24" }}>
            <span className="pill-dot" />
            <span>{!status ? "…" : !status.installed ? "Module voix incomplet" : status.running ? "Serveur voix lancé" : "Serveur voix arrêté"}</span>
            {status && !status.installed && <span className="muted small">— manque : {status.missing.join(", ")}</span>}
            {status?.installed && (
              <button className="glass-btn sm" onClick={startStop} disabled={busy}>
                {status.running ? <><I.x size={11} /> Arrêter</> : <><I.power size={11} /> Démarrer</>}
              </button>
            )}
            <button className="glass-btn sm" onClick={async () => setLog(await api.voiceLog())}>Journal</button>
          </div>
          {status && !status.installed && (
            <p className="muted small">Installation : <code>powershell -ExecutionPolicy Bypass -File voix\installer.ps1</code> depuis le dossier du projet (environ 12 Go).</p>
          )}
          {log && <pre className="voice-log">{log}</pre>}
          {latency && (
            <p className="small">
              Dernière réponse : <b>{(latency.total_ms / 1000).toFixed(2)} s</b> entre la fin de ta phrase et le premier son
              <span className="muted"> (attente de fin de phrase {latency.vad_ms} ms · écoute {latency.stt_ms ?? "?"} ms · cerveau {latency.llm_ms ?? "?"} ms · voix {latency.tts_ms ?? "?"} ms)</span>
            </p>
          )}

          <div className="field-grid">
            <label>
              <span>Façon de parler</span>
              <select value={form.voice_mode} onChange={(e) => set("voice_mode", e.target.value as Config["voice_mode"])}>
                <option value="mains-libres">Mains libres (je détecte la fin de ta phrase)</option>
                <option value="appuyer">Appuyer pour parler</option>
              </select>
            </label>
            {form.voice_mode === "appuyer" && (
              <label><span>Touche à maintenir</span><input value={form.voice_ptt_key} onChange={(e) => set("voice_ptt_key", e.target.value)} placeholder="F2" /></label>
            )}
          </div>
          <label className="toggle">
            <input type="checkbox" checked={form.voice_barge_in} onChange={(e) => set("voice_barge_in", e.target.checked)} />
            <span>Pouvoir couper la parole à Orchestra en parlant</span>
            <span className="muted small">Le micro reste ouvert pendant qu'elle parle. Sans casque, si elle se coupe elle-même, décoche cette option.</span>
          </label>
        </>
      )}
    </section>
  );
}
