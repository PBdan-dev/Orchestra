import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { api } from "../tauri";
import type { ChampAgent, ConfigurationRequise } from "../types";
import { I } from "./Icons";

/**
 * Champs dont un agent a besoin pour fonctionner (retour de PB du 03/11) : identifiants rangés dans
 * le coffre de Windows (la valeur n'est jamais réaffichée ni écrite ailleurs) et réglages simples,
 * comme une adresse Gmail. Utilisé dans le panneau Agents, l'onglet Agents et le formulaire
 * qui s'ouvre quand une mission est bloquée.
 */

const LIBELLES: Record<string, string> = {
  mot_de_passe_application: "mot de passe d'application",
  cle_api: "clé d'API",
  jeton: "jeton",
  jeton_oauth: "jeton OAuth",
  texte: "réglage",
};
const joli = (nom: string) => nom.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

function ChampSaisie({ c, valeur, changer }: { c: ChampAgent; valeur: string; changer: (v: string) => void }) {
  const secret = c.genre === "identifiant";
  return (
    <label className={`ac-champ ${c.present ? "present" : c.requis ? "requis" : ""}`}>
      <span className="ac-tete">
        <span className="ac-nom">{joli(c.nom)}</span>
        <span className="ac-type">{LIBELLES[c.type] ?? c.type}</span>
        {c.present ? <span className="tag ok">{secret ? "rangé dans le coffre" : "renseigné"}</span> : c.requis ? <span className="tag requis">requis</span> : <span className="tag">facultatif</span>}
      </span>
      {c.raison && <span className="ac-raison">{c.raison}</span>}
      <input type={secret ? "password" : "text"} autoComplete="off" spellCheck={false} value={valeur}
        placeholder={c.present ? (secret ? "remplacer la valeur…" : c.valeur) : secret ? "valeur, rangée directement dans le coffre de Windows" : "valeur"}
        onChange={(e) => changer(e.target.value)} />
    </label>
  );
}

/** Enregistre les valeurs saisies : identifiants dans le coffre, réglages en clair. */
async function enregistrer(agentId: string, champs: ChampAgent[], valeurs: Record<string, string>) {
  const reglages: Record<string, string> = {}, identifiants: Record<string, string> = {};
  for (const c of champs) {
    const v = (valeurs[c.nom] ?? "").trim();
    if (!v) continue;
    if (c.genre === "identifiant") identifiants[c.nom] = v; else reglages[c.nom] = v;
  }
  await api.enregistrerConfiguration(agentId, reglages, identifiants);
}

/** Champs d'un agent installé, avec leur état et la possibilité de les remplir. */
export function ChampsAgent({ agentId, ouvert = false }: { agentId: string; ouvert?: boolean }) {
  const [champs, setChamps] = useState<ChampAgent[]>([]);
  const [valeurs, setValeurs] = useState<Record<string, string>>({});
  const [voir, setVoir] = useState(ouvert);
  const [msg, setMsg] = useState<{ ok: boolean; t: string } | null>(null);
  const [prog, setProg] = useState<string | null>(null);
  const charger = () => api.champsAgent(agentId).then(setChamps).catch(() => setChamps([]));
  useEffect(() => { charger(); api.configurateurAgent(agentId).then(setProg).catch(() => setProg(null)); }, [agentId]);
  useEffect(() => { if (ouvert) setVoir(true); }, [ouvert]);
  if (champs.length === 0 && !prog) return null;
  const manquants = champs.filter((c) => c.requis && !c.present).length;
  const saisis = Object.values(valeurs).some((v) => v.trim());
  const valider = async () => {
    setMsg(null);
    try {
      await enregistrer(agentId, champs, valeurs);
      setValeurs({});
      await charger();
      setMsg({ ok: true, t: "Enregistré." });
    } catch (e) { setMsg({ ok: false, t: String(e).replace(/^Error: /, "") }); }
  };
  return (
    <div className="ac-bloc">
      <button className={`ac-bascule ${manquants ? "warn" : ""}`} onClick={() => setVoir(!voir)}>
        <I.alert size={11} /> {manquants ? `${manquants} champ(s) requis à renseigner` : champs.length ? `Réglages et identifiants (${champs.length})` : "Configuration de l'agent"}
        <I.chevron size={11} className={voir ? "ac-ouvert" : ""} />
      </button>
      {voir && (
        <div className="ac-liste">
          {champs.map((c) => <ChampSaisie key={c.nom} c={c} valeur={valeurs[c.nom] ?? ""} changer={(v) => setValeurs((x) => ({ ...x, [c.nom]: v }))} />)}
          <div className="ac-actions">
            <button className="glass-btn sm active" disabled={!saisis} onClick={valider}><I.save size={11} /> Enregistrer</button>
            <span className="muted small">Les secrets vont dans le coffre de Windows (orchestra/{agentId}/…), jamais dans un fichier ni dans le chat.</span>
          </div>
          {prog && <BoutonConfigurateur agentId={agentId} prog={prog} apres={charger} />}
          {msg && <div className={`small ${msg.ok ? "ok" : "warn"}`}>{msg.t}</div>}
        </div>
      )}
    </div>
  );
}

/** Lance le programme de configuration livré avec l'agent (adresses des appareils, appairage…) dans une console. */
function BoutonConfigurateur({ agentId, prog, apres }: { agentId: string; prog: string; apres?: () => void }) {
  const [etat, setEtat] = useState<"" | "lance" | string>("");
  const lancer = async () => {
    try { await api.configurerAgent(agentId); setEtat("lance"); setTimeout(() => apres?.(), 4000); } catch (e) { setEtat(String(e).replace(/^Error: /, "")); }
  };
  return (
    <div className="ac-actions">
      <button className="glass-btn sm" onClick={lancer}><I.gear size={11} /> Lancer sa configuration ({prog})</button>
      <span className={`small ${etat && etat !== "lance" ? "warn" : "muted"}`}>{etat === "lance" ? "Fenêtre ouverte : réponds à ses questions, puis reviens ici." : etat || "Règle ce que l'agent doit connaître de ton installation (adresses, appairage)."}</span>
    </div>
  );
}

/** Formulaire ouvert tout seul quand une mission est bloquée faute de réglages ou d'identifiants. */
export function FormulaireConfiguration() {
  const configurations = useStore((s) => s.configurations);
  const fermer = useStore((s) => s.fermerConfiguration);
  const c: ConfigurationRequise | undefined = configurations[0];
  const [valeurs, setValeurs] = useState<Record<string, string>>({});
  const [erreur, setErreur] = useState("");
  const [occupe, setOccupe] = useState(false);
  useEffect(() => { setValeurs({}); setErreur(""); }, [c?.run_id]);
  const manque = c ? c.champs.filter((x) => x.requis && !x.present && !(valeurs[x.nom] ?? "").trim()) : [];
  const valider = async (relancer: boolean) => {
    if (!c) return;
    setOccupe(true); setErreur("");
    try {
      await enregistrer(c.agent_id, c.champs, valeurs);
      fermer(c.run_id, relancer);
    } catch (e) { setErreur(String(e).replace(/^Error: /, "")); } finally { setOccupe(false); }
  };
  return (
    <AnimatePresence>
      {c && (
        <motion.div className="ac-voile" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
          <motion.div className="ac-fenetre glass" role="dialog" aria-modal="true" initial={{ y: 14, scale: 0.98 }} animate={{ y: 0, scale: 1 }} exit={{ y: 10, opacity: 0 }}
            onKeyDown={(e) => { if (e.key === "Escape") fermer(c.run_id, false); }}>
            <header className="ac-fenetre-tete">
              <div>
                <h3><I.alert size={14} /> {c.agent_nom} {c.champs.length ? "a besoin de quelques réglages" : "doit être configuré sur cet ordinateur"}</h3>
                <p className="muted small">{c.champs.length ? "La mission s'est arrêtée" : "L'agent a répondu"} : {c.motif}</p>
              </div>
              <button className="icon-btn" onClick={() => fermer(c.run_id, false)} title="Plus tard"><I.x size={13} /></button>
            </header>
            {c.configurateur && <BoutonConfigurateur agentId={c.agent_id} prog={c.configurateur} />}
            <div className="ac-liste">
              {c.champs.map((x) => <ChampSaisie key={x.nom} c={x} valeur={valeurs[x.nom] ?? ""} changer={(v) => setValeurs((y) => ({ ...y, [x.nom]: v }))} />)}
            </div>
            <p className="muted small">Les mots de passe, codes et clés vont directement dans le coffre de Windows : ils ne passent ni par le chat, ni par le cerveau, ni par le journal.</p>
            {erreur && <div className="small warn">{erreur}</div>}
            <footer className="ac-fenetre-pied">
              <button className="link-btn" onClick={() => fermer(c.run_id, false)}>Plus tard</button>
              <button className="glass-btn sm" disabled={occupe} onClick={() => valider(false)}>Enregistrer seulement</button>
              <button className="glass-btn sm active" disabled={occupe || manque.length > 0} onClick={() => valider(true)}
                title={manque.length ? `Il manque : ${manque.map((x) => joli(x.nom)).join(", ")}` : "Enregistre, puis relance ta demande"}>
                <I.play size={11} /> Enregistrer et relancer « {c.demande.length > 40 ? c.demande.slice(0, 40) + "…" : c.demande} »
              </button>
            </footer>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
