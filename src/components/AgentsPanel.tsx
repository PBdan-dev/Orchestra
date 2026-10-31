import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useStore } from "../store";
import { api } from "../tauri";
import type { Catalogue, EtatConfiance, Identifiant, Installation, Manifest } from "../types";
import { AgentGlyph, I } from "./Icons";

/**
 * Panneau modulaire des agents : chaque agent est indépendant. On peut l'installer,
 * le brancher sur la scène, le débrancher ou le désinstaller sans toucher aux autres.
 * Désinstaller retire seulement son manifeste : sa mémoire reste dans memoire/agents/.
 */
export function AgentsPanel() {
  const registry = useStore((s) => s.registry);
  const installed = useStore((s) => s.installed);
  const agents = useStore((s) => s.agents);
  const plug = useStore((s) => s.plug);
  const unplug = useStore((s) => s.unplug);
  const installAgent = useStore((s) => s.installAgent);
  const uninstallAgent = useStore((s) => s.uninstallAgent);
  const setAgentsPanel = useStore((s) => s.setAgentsPanel);
  const autorisations = useStore((s) => s.autorisations);
  const retirerAutorisation = useStore((s) => s.retirerAutorisation);
  const reinitialiserMemoire = useStore((s) => s.reinitialiserMemoire);
  const refreshAutorisations = useStore((s) => s.refreshAutorisations);
  useEffect(() => { refreshAutorisations().catch(() => {}); }, [refreshAutorisations]);
  const remettreAZero = async (m: Manifest) => {
    if (!window.confirm(`Remettre à zéro la mémoire de ${m.name} ? L'ancienne mémoire est archivée (agents/memoires-archivees/), pas effacée.`)) return;
    try {
      const archive = await reinitialiserMemoire(m.id);
      window.alert(`Mémoire de ${m.name} remise à zéro. Ancienne mémoire : ${archive}`);
    } catch (e) {
      window.alert(String(e));
    }
  };
  const PORTEE: Record<string, string> = { type_session: "pour la session", tout_session: "tout, pour la session", permanent: "tout, permanent" };
  const [busyId, setBusyId] = useState<string | null>(null);
  const installAgentZip = useStore((s) => s.installAgentZip);
  const modeDev = useStore((s) => s.config?.mode_developpement ?? false);
  // Agent Market (lot T4)
  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [erreurCatalogue, setErreurCatalogue] = useState("");
  const [message, setMessage] = useState<{ ok: boolean; texte: string } | null>(null);
  const [ouvrirIds, setOuvrirIds] = useState<string | null>(null);
  const chargerCatalogue = async () => {
    setErreurCatalogue("");
    try { setCatalogue(await api.catalogue()); } catch (e) { setErreurCatalogue(String(e)); }
  };
  useEffect(() => { chargerCatalogue(); }, [installed.length, modeDev]);
  const apres = (i: Installation) => {
    const manquants = i.identifiants.filter((x) => !x.present).length;
    const quoi = i.ancienne_version ? `mis à jour (${i.ancienne_version} → ${i.manifest.version})` : `installé (${i.manifest.version})`;
    setMessage({ ok: true, texte: `${i.manifest.name} ${quoi}.${manquants ? ` ${manquants} identifiant(s) à renseigner ci-dessous.` : ""}` });
    if (manquants) setOuvrirIds(i.manifest.id);
    chargerCatalogue();
  };
  const installer = async (id: string) => {
    setMessage(null);
    try { apres(await installAgent(id)); } catch (e) { setMessage({ ok: false, texte: `Installation refusée : ${String(e)}` }); }
  };
  const installerZip = async () => {
    const chemin = window.prompt("Chemin complet du paquet .zip de l'agent (mode développement, badge « Non certifié ») :");
    if (!chemin || !chemin.trim()) return;
    setMessage(null);
    try { apres(await installAgentZip(chemin.trim())); } catch (e) { setMessage({ ok: false, texte: `Installation refusée : ${String(e)}` }); }
  };
  const aInstaller = (catalogue?.entrees ?? []).filter((c) => c.installe !== c.manifest.version);
  // Confiance (lot T5)
  const [confiance, setConfiance] = useState<Record<string, EtatConfiance>>({});
  const chargerConfiance = () => api.confianceAgents().then(setConfiance).catch(() => {});
  useEffect(() => { chargerConfiance(); }, [installed.length]);
  const lever = async (m: Manifest) => {
    if (!window.confirm(`Lever la quarantaine de ${m.name} ? Son compteur d'écarts repart à zéro ; Orchestra pourra de nouveau le choisir, mais sans le considérer « de confiance » tant que tu ne l'as pas rétabli.`)) return;
    try { await api.leverQuarantaine(m.id); } catch (e) { window.alert(String(e)); }
    chargerConfiance();
  };
  const retablir = async (m: Manifest) => {
    try { await api.retablirConfiance(m.id); } catch (e) { window.alert(String(e)); }
    chargerConfiance();
  };

  const all: Manifest[] = [...installed, ...registry.filter((r) => !installed.some((i) => i.id === r.id))];
  const wrap = async (id: string, fn: () => Promise<void>) => { setBusyId(id); try { await fn(); } finally { setBusyId(null); } };

  return (
    <motion.aside className="agents-panel glass" initial={{ opacity: 0, y: -10, scale: 0.98 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: -10, scale: 0.98 }} transition={{ duration: 0.2 }}>
      <header className="ap-head">
        <span><I.grid size={13} /> Agents disponibles</span>
        <button className="icon-btn" onClick={() => setAgentsPanel(false)}><I.x size={14} /></button>
      </header>
      <p className="ap-note">Chaque agent est un module indépendant. Brancher ou débrancher ne touche ni aux autres agents ni à la mémoire.</p>
      <div className="ap-list">
        {all.length === 0 && <p className="muted small">Aucun agent. Ajoute-en dans agents/registre-local.json.</p>}
        {all.map((m) => {
          const isInstalled = installed.some((i) => i.id === m.id);
          const onStage = !!agents[m.id];
          const c = m.color || "#818cf8";
          return (
            <div key={m.id} className={`ap-row ${onStage ? "on" : ""}`} style={{ ["--c" as any]: c }}>
              <span className="ap-glyph"><AgentGlyph icon={m.icon} size={15} /></span>
              <div className="ap-info">
                <div className="ap-name">
                  {m.name} <span className="muted small">{m.version}</span> {isInstalled ? <span className="tag ok">installé</span> : <span className="tag">registre</span>}{" "}
                  {isInstalled && <BadgeCertification statut={m.certification} />}{" "}
                  {confiance[m.id]?.statut === "quarantaine" && <span className="tag quarantaine" title="Orchestra ne le choisit plus">en quarantaine · {confiance[m.id].ecarts} écarts</span>}
                  {confiance[m.id]?.statut === "retire" && <span className="tag retire" title="Écart relevé lors d'une mission : il n'est plus « de confiance »">confiance retirée</span>}
                </div>
                <div className="ap-desc">{m.description}</div>
                <div className="ap-tools">{m.tools.map((t) => <span key={t} className="mini">{t}</span>)}</div>
                {m.avertissement && <div className="ap-avert"><I.alert size={11} /> {m.avertissement}</div>}
                {confiance[m.id]?.statut === "quarantaine" && <button className="link-btn" onClick={() => lever(m)}>lever la quarantaine</button>}
                {confiance[m.id]?.statut === "retire" && <button className="link-btn" onClick={() => retablir(m)}>rétablir la confiance</button>}
                {isInstalled && <IdentifiantsAgent agent={m} ouvert={ouvrirIds === m.id} />}
              </div>
              <div className="ap-actions">
                <button className={`glass-btn sm ${onStage ? "active" : ""}`} onClick={() => (onStage ? unplug(m.id) : plug(m))}>
                  {onStage ? <><I.unplug size={11} /> Débrancher</> : <><I.plug size={11} /> Brancher</>}
                </button>
                {isInstalled && <button className="link-btn" disabled={busyId === m.id} onClick={() => wrap(m.id, () => remettreAZero(m))}>remettre sa mémoire à zéro</button>}
                {isInstalled ? (
                  <button className="link-btn danger" disabled={busyId === m.id} onClick={() => wrap(m.id, () => uninstallAgent(m.id))}>désinstaller</button>
                ) : (
                  <button className="link-btn" disabled={busyId === m.id} onClick={() => wrap(m.id, () => installer(m.id))}>installer</button>
                )}
              </div>
            </div>
          );
        })}
      </div>
      <div className="ap-section">
        <I.plug size={12} /> Agent Market <span className="muted small">· {catalogue ? `${catalogue.entrees.length} agent(s)` : "…"}</span>
        <button className="link-btn right" onClick={chargerCatalogue}><I.refresh size={11} /> actualiser</button>
      </div>
      <div className="ap-list">
        {message && <div className={`small ${message.ok ? "ok" : "warn"}`} style={{ padding: "2px 4px" }}>{message.texte}</div>}
        {erreurCatalogue && <div className="small warn" style={{ padding: "2px 4px" }}>{erreurCatalogue}</div>}
        {catalogue && aInstaller.length === 0 && !erreurCatalogue && <p className="muted small">Rien de nouveau à installer{catalogue.entrees.length === 0 ? " : le catalogue est vide" : ""}.</p>}
        {catalogue && catalogue.ignores.length > 0 && <div className="ap-motif" title={catalogue.ignores.join("\n")}>{catalogue.ignores.length} entrée(s) du catalogue ignorée(s) car invalides (survoler pour le motif)</div>}
        {aInstaller.map((c) => (
          <div key={c.manifest.id} className="ap-row" style={{ ["--c" as any]: c.manifest.color || "#818cf8" }}>
            <span className="ap-glyph"><AgentGlyph icon={c.manifest.icon} size={15} /></span>
            <div className="ap-info">
              <div className="ap-name">{c.manifest.name} <span className="muted small">{c.manifest.version}</span> <BadgeCertification statut={c.installable && c.motif ? "non_certifie" : c.manifest.certification} /></div>
              <div className="ap-desc">{c.manifest.description}</div>
              {!c.installable && <div className="ap-motif">{c.motif}</div>}
            </div>
            <div className="ap-actions">
              <button className="glass-btn sm" disabled={!c.installable || busyId === c.manifest.id} onClick={() => wrap(c.manifest.id, () => installer(c.manifest.id))}>
                {busyId === c.manifest.id ? "vérification…" : c.installe ? `Mettre à jour (${c.installe} → ${c.manifest.version})` : "Installer"}
              </button>
            </div>
          </div>
        ))}
        {modeDev && <button className="link-btn" onClick={installerZip}><I.save size={11} /> installer depuis un fichier .zip (mode développement)</button>}
      </div>
      <div className="ap-autorisations">
        <div><I.alert size={12} /> Actions irréversibles autorisées sans question</div>
        {autorisations.length === 0 && <div className="muted small">aucune : chaque action irréversible te sera demandée</div>}
        {autorisations.map((a) => (
          <div key={`${a.agent_id}-${a.action}-${a.portee}`} className="ap-aut">
            <span>{a.agent_id} · {a.action ? <code>{a.action}</code> : "toutes"} · <span className="muted">{PORTEE[a.portee] ?? a.portee}</span></span>
            <button className="link-btn danger" onClick={() => retirerAutorisation(a)}>retirer</button>
          </div>
        ))}
      </div>
      <footer className="ap-foot">
        <button className="link-btn" onClick={() => api.openFolder("agents")}><I.external size={11} /> ouvrir le dossier agents</button>
      </footer>
    </motion.aside>
  );
}

/** Badge de certification (D25). */
function BadgeCertification({ statut }: { statut?: string }) {
  if (statut === "orchestra_certified") return <span className="tag certifie" title="Vérifié par l'Agent Market">Orchestra Certified</span>;
  if (statut === "livrable_seul") return <span className="tag" title="Agent hébergé : seul le livrable est contrôlé">Livrable seul</span>;
  return <span className="tag noncertifie" title="Non vérifié par l'Agent Market (mode développement)">Non certifié</span>;
}

/**
 * Identifiants déclarés par l'agent (D34) : demandés à PB avec leur raison, rangés dans le
 * Gestionnaire d'identification de Windows sous orchestra/<id>/<nom>. La valeur n'est jamais réaffichée.
 */
function IdentifiantsAgent({ agent, ouvert }: { agent: Manifest; ouvert: boolean }) {
  const [ids, setIds] = useState<Identifiant[]>([]);
  const [valeurs, setValeurs] = useState<Record<string, string>>({});
  const [voir, setVoir] = useState(ouvert);
  const [erreur, setErreur] = useState("");
  const charger = () => api.identifiantsAgent(agent.id).then(setIds).catch(() => setIds([]));
  useEffect(() => { charger(); }, [agent.id, agent.version]);
  useEffect(() => { if (ouvert) setVoir(true); }, [ouvert]);
  if (ids.length === 0) return null;
  const manquants = ids.filter((i) => !i.present).length;
  const enregistrer = async (nom: string) => {
    setErreur("");
    try {
      await api.enregistrerIdentifiant(agent.id, nom, valeurs[nom] ?? "");
      setValeurs((v) => ({ ...v, [nom]: "" }));
      await charger();
    } catch (e) {
      setErreur(String(e));
    }
  };
  return (
    <div className="ap-identifiants">
      <button className="link-btn" onClick={() => setVoir(!voir)} style={{ alignSelf: "flex-start" }}>
        <I.alert size={11} /> {manquants ? `${manquants} identifiant(s) à renseigner` : `identifiants rangés (${ids.length})`} {voir ? "· replier" : "· voir"}
      </button>
      {voir && ids.map((i) => (
        <div key={i.nom} className="ap-id">
          <code>{i.nom}</code> <span className="muted small">({i.type}) {i.raison}</span>
          {i.present && <span className="tag ok">rangé</span>}
          <input type="password" autoComplete="off" placeholder={i.present ? "remplacer la valeur…" : "valeur"} value={valeurs[i.nom] ?? ""} onChange={(e) => setValeurs((v) => ({ ...v, [i.nom]: e.target.value }))} />
          <button className="glass-btn sm" disabled={!(valeurs[i.nom] ?? "")} onClick={() => enregistrer(i.nom)}>Ranger dans le coffre de Windows</button>
        </div>
      ))}
      {voir && <div className="muted small">Rangé sous orchestra/{agent.id}/&lt;nom&gt;, jamais dans un fichier ni dans les traces.</div>}
      {erreur && <div className="small warn">{erreur}</div>}
    </div>
  );
}
