import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../tauri";
import type { MemoireGraphique, MissionResume } from "../types";
import { I } from "./Icons";

const ETATS: Record<string, string> = {
  termine: "terminée", partiel: "partielle", echec: "échec", bloque: "bloquée", precision_requise: "précision demandée",
  arrete_proprement: "arrêtée proprement", arrete_force: "arrêtée de force", en_pause: "en pause", en_cours: "en cours", inconnu: "—",
};

/**
 * Vue « Missions » (lot T7) : toutes les missions des agents, de la plus récente à la plus
 * ancienne, avec leur dossier ; mémoire de la carte graphique et mode d'exécution (série, D17).
 */
export function MissionsView() {
  const [missions, setMissions] = useState<MissionResume[]>([]);
  const [gpu, setGpu] = useState<MemoireGraphique | null>(null);
  const [gpuErreur, setGpuErreur] = useState("");
  const reprendre = useStore((s) => s.reprendreMission);
  const busy = useStore((s) => s.busy);
  const setView = useStore((s) => s.setView);
  const charger = () => {
    api.listerMissions().then(setMissions).catch(() => setMissions([]));
    api.memoireGraphique().then(setGpu).catch((e) => setGpuErreur(String(e)));
  };
  useEffect(() => { charger(); }, [busy]);
  const date = (d: string) => (d ? new Date(d).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" }) : "");
  return (
    <div className="missions-view">
      <div className="entete">
        <h2 style={{ margin: 0 }}><I.folder size={16} /> Missions</h2>
        <span className="chip" title="Une mission à la fois sur la carte graphique ; pas de conversation pendant le travail (D7)">Mode série</span>
        {gpu && (
          <span className="small">
            {gpu.detail} · <span className="barre"><span style={{ width: `${Math.round((gpu.utilisee / Math.max(gpu.totale, 1)) * 100)}%` }} /></span>{" "}
            {(gpu.utilisee / 1024).toFixed(1)} / {(gpu.totale / 1024).toFixed(1)} Go utilisés
          </span>
        )}
        {gpuErreur && <span className="small muted">{gpuErreur}</span>}
        <button className="link-btn" onClick={charger}><I.refresh size={11} /> actualiser</button>
      </div>
      {missions.length === 0 && <p className="muted">Aucune mission pour l'instant.</p>}
      {missions.length > 0 && (
        <table>
          <thead><tr><th>Date</th><th>Projet</th><th>Agent</th><th>Mission</th><th>État</th><th>Vérification</th><th /></tr></thead>
          <tbody>
            {missions.map((m) => (
              <tr key={m.id}>
                <td>{date(m.date)}</td>
                <td>{m.projet}</td>
                <td>{m.agent_id}</td>
                <td>{m.but}</td>
                <td>{ETATS[m.etat] ?? m.etat}</td>
                <td>{m.ecarts === null ? <span className="muted">—</span> : m.ecarts === 0 ? <span className="ok">aucun écart</span> : <span className="warn">{m.ecarts} écart(s)</span>}</td>
                <td style={{ whiteSpace: "nowrap" }}>
                  <button className="link-btn" onClick={() => { api.verifierIntegrite(m.id).catch(() => {}); api.openFolder(`mission:${m.id}`); }}><I.external size={11} /> ouvrir</button>
                  {m.reprenable && (
                    <button className="link-btn" disabled={busy} onClick={async () => { try { await reprendre(m.id); setView("stage"); } catch (e) { window.alert(String(e)); } }}>
                      <I.play size={11} /> reprendre
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
