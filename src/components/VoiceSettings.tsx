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
        Parle à Orchestra et elle te répond à l'oral. Tout tourne sur ton PC : Voxtral pour écouter, {form.voice_llm || "Qwen 3.5 9B"} pour
        répondre vite, Qwen3-TTS pour la voix, reliés par Pipecat. Un travail concret dit à voix haute passe par la même boucle que le chat.
      </p>

      <p className="muted small">Le passage du chat à la voix se fait avec le switch sous le cœur. Réglages avancés :</p>

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
            <label>
              <span>Cerveau du mode vocal <span className="muted">(rapide)</span></span>
              <input value={form.voice_llm} onChange={(e) => set("voice_llm", e.target.value)} placeholder="qwen3.5:9b" />
            </label>
            <label>
              <span>Voix de Qwen3-TTS</span>
              <select value={form.voice_speaker} onChange={(e) => set("voice_speaker", e.target.value)} disabled={!!form.voice_reference}>
                {SPEAKERS.map((s) => <option key={s} value={s}>{s}</option>)}
              </select>
            </label>
            <label>
              <span>Voix clonée <span className="muted">(fichier de voix\references, avec l'accord de la personne)</span></span>
              <input value={form.voice_reference} onChange={(e) => set("voice_reference", e.target.value)} placeholder="vide = voix ci-dessus" />
            </label>
          </div>
          <label className="toggle">
            <input type="checkbox" checked={form.voice_read_chat} onChange={(e) => set("voice_read_chat", e.target.checked)} />
            <span>Lire aussi à voix haute les réponses aux messages tapés</span>
            <span className="muted small">Quand le mode vocal est ouvert.</span>
          </label>
          <label className="toggle">
            <input type="checkbox" checked={form.voice_barge_in} onChange={(e) => set("voice_barge_in", e.target.checked)} />
            <span>Pouvoir couper la parole à Orchestra en parlant</span>
            <span className="muted small">Le micro reste ouvert pendant qu'elle parle. Sans casque, si elle se coupe elle-même, décoche cette option.</span>
          </label>
          <label className="toggle">
            <input type="checkbox" checked={form.voice_autostart} onChange={(e) => set("voice_autostart", e.target.checked)} />
            <span>Préparer la voix au lancement d'Orchestra</span>
            <span className="muted small">Les modèles se chargent en arrière-plan (environ 10 Go de mémoire GPU), le micro reste fermé.</span>
          </label>
          <p className="muted small">Changer la voix ou le cerveau vocal : enregistrer, puis Arrêter et Démarrer le serveur voix.</p>
        </>
      )}
    </section>
  );
}
