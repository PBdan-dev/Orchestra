import { useEffect, useState } from "react";
import { useStore } from "../store";
import { brainStatus } from "../brainStatus";
import { I } from "./Icons";

/** Voix préréglées du modèle Qwen3-TTS CustomVoice. */
const SPEAKERS = ["Ryan", "Aiden", "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ono_Anna", "Sohee"];

const pretty = {
  stt: (f: string) => f.replace(/\.gguf$/, "").replace("Voxtral-Mini-4B-Realtime-2602", "Voxtral Mini 4B Realtime").replace(/-/g, " "),
  tts: (d: string) => d.replace("Qwen3-TTS-12Hz-", "Qwen3-TTS ").replace("-CustomVoice", " · voix préréglées").replace("-Base", " · voix clonées"),
  ref: (f: string) => f.replace(/\.(wav|mp3|flac|m4a)$/i, ""),
};

/**
 * Les modèles d'Orchestra, mode par mode, chacun avec sa liste déroulante.
 * Le groupe du mode actif est mis en avant ; le switch du cœur charge le bon cerveau.
 */
export function ModelsPanel() {
  const config = useStore((s) => s.config);
  const brain = useStore((s) => s.brain);
  const brainLoading = useStore((s) => s.brainLoading);
  const voiceOn = useStore((s) => s.voiceOn);
  const models = useStore((s) => s.voiceModels);
  const switching = useStore((s) => s.switching);
  const error = useStore((s) => s.voiceError);
  const refreshVoiceModels = useStore((s) => s.refreshVoiceModels);
  const refreshBrain = useStore((s) => s.refreshBrain);
  const setChatModel = useStore((s) => s.setChatModel);
  const setVoiceLlm = useStore((s) => s.setVoiceLlm);
  const setSttModel = useStore((s) => s.setSttModel);
  const setTtsModel = useStore((s) => s.setTtsModel);
  const setVoice = useStore((s) => s.setVoice);
  const [open, setOpen] = useState<string | null>(null);

  useEffect(() => { refreshVoiceModels(); refreshBrain(); }, []);
  if (!config) return null;

  const llms = brain?.models ?? [];
  const status = brainStatus(brain, brainLoading);
  const chatModel = config.model || brain?.model || "";
  const tts = config.voice_tts_model;
  const isClone = !!config.voice_reference;
  const currentVoice = isClone ? `ref:${config.voice_reference}` : config.voice_speaker;
  const hasBase = models?.tts.some((t) => t.includes("Base"));

  const pick = (id: string, fn: (v: string) => void) => (v: string) => { setOpen(null); fn(v); };

  return (
    <>
      {switching && <p className="small" style={{ color: "#5eead4" }}>{switching}</p>}
      {error && <p className="small warn">{error}</p>}

      <div className={`model-group ${!voiceOn ? "active" : ""}`}>
        <div className="model-group-head"><I.chat size={12} /> Mode chat {!voiceOn && <span className="chip tiny">actif</span>}</div>
        <ModelRow id="chat" label="Cerveau" value={chatModel || "auto"} sub={!voiceOn ? status.label : "chargé au passage en chat"}
          open={open} setOpen={setOpen} options={llms.map((m) => ({ value: m, label: m }))} current={chatModel}
          onPick={pick("chat", setChatModel)} empty="Aucun modèle Ollama trouvé" />
        <p className="small muted">Réflexion activée : réponses plus lentes, plus soignées.</p>
      </div>

      <div className={`model-group ${voiceOn ? "active" : ""}`}>
        <div className="model-group-head"><I.mic size={12} /> Mode voix {voiceOn && <span className="chip tiny">actif</span>}</div>
        <ModelRow id="stt" label="Écoute" value={pretty.stt(config.voice_stt_model)} open={open} setOpen={setOpen}
          options={(models?.stt ?? []).map((f) => ({ value: f, label: pretty.stt(f) }))} current={config.voice_stt_model}
          onPick={pick("stt", setSttModel)} empty="Aucun modèle d'écoute installé" note="Changer recharge la voix (≈ 30 s)." />
        <ModelRow id="voicellm" label="Cerveau" value={config.voice_llm} open={open} setOpen={setOpen}
          options={llms.map((m) => ({ value: m, label: m }))} current={config.voice_llm}
          onPick={pick("voicellm", setVoiceLlm)} empty="Aucun modèle Ollama trouvé" note="Un petit modèle répond plus vite. Réflexion seulement quand la question l'exige." />
        <ModelRow id="tts" label="Synthèse" value={pretty.tts(tts)} open={open} setOpen={setOpen}
          options={(models?.tts ?? []).map((d) => ({ value: d, label: pretty.tts(d) }))} current={tts}
          onPick={pick("tts", setTtsModel)} empty="Aucun modèle de synthèse installé" note="Changer recharge la voix (≈ 30 s)." />
        <ModelRow id="voice" label="Voix" value={isClone ? `${pretty.ref(config.voice_reference)} (clonée)` : config.voice_speaker} open={open} setOpen={setOpen}
          current={currentVoice} onPick={pick("voice", setVoice)} empty=""
          options={[
            ...SPEAKERS.map((s) => ({ value: s, label: s, hint: "préréglée" })),
            ...(models?.references ?? []).map((f) => ({ value: `ref:${f}`, label: pretty.ref(f), hint: hasBase ? "clonée" : "clonée · modèle Base requis" })),
          ]}
          note={`Voix clonées : déposer un enregistrement de 5 à 15 s dans voix\\references, avec l'accord de la personne.${hasBase ? "" : " Il faut aussi le modèle Qwen3-TTS Base."}`} />
      </div>
    </>
  );
}

function ModelRow({ id, label, value, sub, options, current, onPick, open, setOpen, empty, note }: {
  id: string; label: string; value: string; sub?: string;
  options: { value: string; label: string; hint?: string }[]; current: string;
  onPick: (v: string) => void; open: string | null; setOpen: (v: string | null) => void; empty: string; note?: string;
}) {
  const isOpen = open === id;
  return (
    <div className={`model-row ${isOpen ? "open" : ""}`}>
      <button className="model-row-btn" onClick={() => setOpen(isOpen ? null : id)} aria-expanded={isOpen}>
        <span className="model-row-label">{label}</span>
        <span className="model-row-value" title={value}>{value}{sub && <span className="muted small"> · {sub}</span>}</span>
        <I.chevron size={12} className="model-row-chev" />
      </button>
      {isOpen && (
        <div className="model-list" role="listbox">
          {options.length === 0 && <p className="small muted" style={{ padding: 6 }}>{empty}</p>}
          {options.map((o) => (
            <button key={o.value} className={`model-opt ${o.value === current ? "cur" : ""}`} role="option" aria-selected={o.value === current} onClick={() => onPick(o.value)}>
              {o.value === current ? <I.check size={12} /> : <span style={{ width: 12 }} />} {o.label}
              {o.hint && <span className="muted">{o.hint}</span>}
            </button>
          ))}
          {note && <p className="small muted" style={{ padding: "6px 8px 2px" }}>{note}</p>}
        </div>
      )}
    </div>
  );
}
