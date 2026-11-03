import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { api } from "../tauri";
import type { Catalogue, EntreeCatalogue, EtatConfiance, Manifest, UsageAgent } from "../types";
import { BadgeCertification, BadgeExecution } from "./AgentsPanel";
import { ChampsAgent } from "./AgentConfig";
import { AgentGlyph, I } from "./Icons";

/**
 * Onglet Agents : tous les agents qu'Orchestra connaît (installés, catalogue de l'Agent Market,
 * agents déjà utilisés mais absents des deux), avec thèmes, filtres, tri et recherche, pour
 * rester lisible quand il y en aura beaucoup. Chaque carte dit ce qu'Orchestra a déjà fait avec l'agent.
 */

const THEMES: Record<string, string> = {
  documents: "Documents", communication: "Communication", messagerie: "Messagerie", recherche: "Recherche",
  redaction: "Rédaction", donnees: "Données", developpement: "Développement", fichiers: "Fichiers",
  utilitaire: "Utilitaires", agenda: "Agenda", image: "Images", audio: "Audio", finance: "Finance",
};
const nomTheme = (t: string) => THEMES[t] ?? (t ? t.charAt(0).toUpperCase() + t.slice(1) : "Autres");

type Statut = "installe" | "maj" | "a_installer" | "bloque" | "connu";
interface Fiche {
  id: string;
  manifest: Manifest | null;
  /** version installée */
  installe: string | null;
  entree: EntreeCatalogue | null;
  usage: UsageAgent | null;
  statut: Statut;
  theme: string;
}

const STATUTS: { id: "tous" | Statut | "utilises"; label: string }[] = [
  { id: "tous", label: "Tous" },
  { id: "installe", label: "Installés" },
  { id: "a_installer", label: "À installer" },
  { id: "maj", label: "Mises à jour" },
  { id: "utilises", label: "Déjà utilisés" },
];
const TRIS = [
  { id: "nom", label: "Nom" },
  { id: "theme", label: "Thème" },
  { id: "recent", label: "Utilisés récemment" },
  { id: "frequent", label: "Plus utilisés" },
] as const;
type Tri = (typeof TRIS)[number]["id"];

const date = (s: string | null | undefined) => (s ? new Date(s).toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric" }) : "");

export function AgentsView() {
  const installed = useStore((s) => s.installed);
  const agentsScene = useStore((s) => s.agents);
  const installAgent = useStore((s) => s.installAgent);
  const uninstallAgent = useStore((s) => s.uninstallAgent);
  const plug = useStore((s) => s.plug);
  const unplug = useStore((s) => s.unplug);
  const setView = useStore((s) => s.setView);
  const focus = useStore((s) => s.agentFocus);
  const modeDev = useStore((s) => s.config?.mode_developpement ?? false);

  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [erreurCatalogue, setErreurCatalogue] = useState("");
  const [usages, setUsages] = useState<UsageAgent[]>([]);
  const [confiance, setConfiance] = useState<Record<string, EtatConfiance>>({});
  const [theme, setTheme] = useState<string>("tous");
  const [statut, setStatut] = useState<(typeof STATUTS)[number]["id"]>("tous");
  const [tri, setTri] = useState<Tri>("nom");
  const [q, setQ] = useState("");
  const [choisi, setChoisi] = useState<string | null>(focus);
  const [occupe, setOccupe] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; texte: string } | null>(null);
  const [voirIgnores, setVoirIgnores] = useState(false);
  const [fonctions, setFonctions] = useState(false);

  const charger = async () => {
    setErreurCatalogue("");
    api.catalogue().then(setCatalogue).catch((e) => setErreurCatalogue(String(e).replace(/^Error: /, "")));
    api.agentsUsage().then(setUsages).catch(() => {});
    api.confianceAgents().then(setConfiance).catch(() => {});
  };
  useEffect(() => { charger(); }, [installed.length, modeDev]);
  useEffect(() => { if (focus) setChoisi(focus); }, [focus]);

  const fiches = useMemo<Fiche[]>(() => {
    const m = new Map<string, Fiche>();
    const usage = (id: string) => usages.find((u) => u.agent_id === id) ?? null;
    for (const a of installed) {
      m.set(a.id, { id: a.id, manifest: a, installe: a.version, entree: null, usage: usage(a.id), statut: "installe", theme: a.categorie || "" });
    }
    for (const e of catalogue?.entrees ?? []) {
      const f = m.get(e.manifest.id);
      if (f) {
        f.entree = e;
        if (e.installe && e.installe !== e.manifest.version) f.statut = "maj";
        if (!f.theme) f.theme = e.manifest.categorie || "";
      } else {
        m.set(e.manifest.id, { id: e.manifest.id, manifest: e.manifest, installe: null, entree: e, usage: usage(e.manifest.id), statut: e.installable ? "a_installer" : "bloque", theme: e.manifest.categorie || "" });
      }
    }
    // agents déjà utilisés par Orchestra, mais ni installés ni au catalogue (ex. Mail Adagio lancé à la main)
    for (const u of usages) {
      if (!m.has(u.agent_id)) m.set(u.agent_id, { id: u.agent_id, manifest: null, installe: null, entree: null, usage: u, statut: "connu", theme: "" });
    }
    return [...m.values()];
  }, [installed, catalogue, usages]);

  const themes = useMemo(() => {
    const c = new Map<string, number>();
    for (const f of fiches) c.set(f.theme, (c.get(f.theme) ?? 0) + 1);
    return [...c.entries()].sort((a, b) => nomTheme(a[0]).localeCompare(nomTheme(b[0]), "fr"));
  }, [fiches]);

  const visibles = useMemo(() => {
    const mots = q.trim().toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").split(/\s+/).filter(Boolean);
    const texte = (f: Fiche) => [f.id, f.manifest?.name, f.manifest?.description, ...(f.manifest?.capabilities ?? []), ...(f.manifest?.sait_faire ?? []), nomTheme(f.theme)]
      .join(" ").toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");
    const l = fiches.filter((f) =>
      (theme === "tous" || f.theme === theme) &&
      (statut === "tous" || (statut === "utilises" ? (f.usage?.missions ?? 0) > 0 : statut === "a_installer" ? f.statut === "a_installer" || f.statut === "bloque" : f.statut === statut || (statut === "installe" && f.statut === "maj"))) &&
      mots.every((w) => texte(f).includes(w)));
    const nom = (f: Fiche) => f.manifest?.name ?? f.id;
    l.sort((a, b) => {
      if (tri === "theme") return nomTheme(a.theme).localeCompare(nomTheme(b.theme), "fr") || nom(a).localeCompare(nom(b), "fr");
      if (tri === "recent") return (b.usage?.derniere ?? "").localeCompare(a.usage?.derniere ?? "") || nom(a).localeCompare(nom(b), "fr");
      if (tri === "frequent") return (b.usage?.missions ?? 0) - (a.usage?.missions ?? 0) || nom(a).localeCompare(nom(b), "fr");
      return nom(a).localeCompare(nom(b), "fr");
    });
    return l;
  }, [fiches, theme, statut, tri, q]);

  const compte = {
    installes: fiches.filter((f) => f.statut === "installe" || f.statut === "maj").length,
    market: (catalogue?.entrees ?? []).length,
    aInstaller: fiches.filter((f) => f.statut === "a_installer").length,
    maj: fiches.filter((f) => f.statut === "maj").length,
  };
  const fiche = fiches.find((f) => f.id === choisi) ?? null;

  const installer = async (f: Fiche) => {
    const c = f.entree;
    if (c?.ce_qui_part && !window.confirm(`${c.manifest.name} est un agent hébergé (livrable seul) : il travaille sur un serveur, pas sur ton ordinateur.\n\nCe qui quittera ta machine à chaque mission :\n- adresse : ${c.ce_qui_part.adresse}\n- données : ${c.ce_qui_part.donnees_envoyees}\n- conservation : ${c.ce_qui_part.conservation}\n\nL'installer ?`)) return;
    setOccupe(f.id); setMessage(null);
    try {
      const i = await installAgent(f.id);
      const manquants = i.identifiants.filter((x) => !x.present).length;
      setMessage({ ok: true, texte: `${i.manifest.name} ${i.ancienne_version ? `mis à jour (${i.ancienne_version} → ${i.manifest.version})` : `installé (${i.manifest.version})`}.${manquants ? ` ${manquants} identifiant(s) à renseigner dans sa fiche.` : ""}` });
      setChoisi(f.id);
    } catch (e) {
      setMessage({ ok: false, texte: `Installation refusée : ${String(e).replace(/^Error: /, "")}` });
    } finally { setOccupe(null); charger(); }
  };
  const desinstaller = async (f: Fiche) => {
    if (!window.confirm(`Désinstaller ${f.manifest?.name ?? f.id} ? Sa mémoire reste dans memoire/agents/.`)) return;
    setOccupe(f.id);
    try { await uninstallAgent(f.id); } catch (e) { window.alert(String(e)); } finally { setOccupe(null); charger(); }
  };

  return (
    <div className="agents-view">
      <header className="av-head">
        <div>
          <h2><I.grid size={16} /> Agents</h2>
          <p className="muted small">
            {compte.installes} installé(s) · {compte.market} sur l'Agent Market{compte.aInstaller ? ` · ${compte.aInstaller} à installer` : ""}{compte.maj ? ` · ${compte.maj} mise(s) à jour` : ""}.
            {" "}Orchestra connaît cette liste et propose l'agent qui convient à chaque demande.
          </p>
        </div>
        <div className="av-outils">
          <label className="av-recherche glass">
            <I.search size={13} />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Chercher un agent, une capacité…" />
            {q && <button className="icon-btn" onClick={() => setQ("")} title="Effacer"><I.x size={11} /></button>}
          </label>
          <label className="av-tri">Trier par
            <select value={tri} onChange={(e) => setTri(e.target.value as Tri)}>{TRIS.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}</select>
          </label>
          <button className="glass-btn sm" onClick={charger}><I.refresh size={11} /> Actualiser</button>
        </div>
      </header>

      <div className="av-corps">
        <nav className="av-themes">
          <div className="av-titre-liste">Thèmes</div>
          <button className={`av-theme ${theme === "tous" ? "on" : ""}`} onClick={() => setTheme("tous")}><span>Tous</span><span className="av-n">{fiches.length}</span></button>
          {themes.map(([t, n]) => (
            <button key={t || "_"} className={`av-theme ${theme === t ? "on" : ""}`} onClick={() => setTheme(t)}><span>{nomTheme(t)}</span><span className="av-n">{n}</span></button>
          ))}
          <div className="av-titre-liste">Statut</div>
          {STATUTS.map((s) => (
            <button key={s.id} className={`av-theme ${statut === s.id ? "on" : ""}`} onClick={() => setStatut(s.id)}><span>{s.label}</span></button>
          ))}
        </nav>

        <section className="av-grille-zone">
          {message && <div className={`av-message small ${message.ok ? "ok" : "warn"}`}>{message.texte}</div>}
          {erreurCatalogue && <div className="av-message small warn">Agent Market : {erreurCatalogue}</div>}
          {catalogue && catalogue.ignores.length > 0 && (
            <div className="av-ignores">
              <button className="link-btn" onClick={() => setVoirIgnores(!voirIgnores)}>
                <I.alert size={11} /> {catalogue.ignores.length} entrée(s) du catalogue ignorée(s) : leur fiche ne respecte pas le format attendu {voirIgnores ? "· replier" : "· voir pourquoi"}
              </button>
              {voirIgnores && <>
                <p className="muted small" style={{ margin: "6px 0 0" }}>Orchestra vérifie chaque fiche du catalogue avant de la proposer : une fiche dont un champ manque ou a le mauvais type est écartée, pour ne jamais installer un agent mal décrit. La correction se fait dans le catalogue de l'Agent Market.</p>
                <ul>{catalogue.ignores.map((i) => <li key={i}><code>{i}</code></li>)}</ul>
              </>}
            </div>
          )}
          {visibles.length === 0 && <p className="muted small av-vide">Aucun agent pour ces filtres.</p>}
          <div className="av-grille">
            <AnimatePresence initial={false}>
              {visibles.map((f) => (
                <motion.button layout key={f.id} className={`av-carte ${choisi === f.id ? "on" : ""} ${f.statut}`} style={{ ["--c" as any]: f.manifest?.color || "#818cf8" }}
                  onClick={() => setChoisi(choisi === f.id ? null : f.id)} initial={{ opacity: 0, scale: 0.97 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.97 }} transition={{ duration: 0.15 }}>
                  <div className="av-carte-tete">
                    <span className="ap-glyph"><AgentGlyph icon={f.manifest?.icon ?? "plug"} size={15} /></span>
                    <div className="av-carte-nom">
                      <b>{f.manifest?.name ?? f.id}</b>
                      <span className="muted small">{nomTheme(f.theme)}{f.manifest ? ` · ${f.manifest.version}` : ""}</span>
                    </div>
                    <PastilleStatut f={f} />
                  </div>
                  <p className="av-carte-desc">{f.manifest?.description ?? "Agent déjà utilisé par Orchestra, mais ni installé ni sur l'Agent Market."}</p>
                  <div className="av-traces">
                    {f.manifest && <BadgeCertification statut={f.entree?.manifest.certification ?? f.manifest.certification} />}
                    <Traces f={f} enScene={!!agentsScene[f.id]} />
                  </div>
                </motion.button>
              ))}
            </AnimatePresence>
          </div>
        </section>

        <AnimatePresence>
          {fiche && (
            <motion.aside key={fiche.id} className="av-fiche glass" style={{ ["--c" as any]: fiche.manifest?.color || "#818cf8" }} initial={{ opacity: 0, x: 16 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 16 }} transition={{ duration: 0.18 }}>
              <div className="av-fiche-tete">
                <span className="ap-glyph"><AgentGlyph icon={fiche.manifest?.icon ?? "plug"} size={17} /></span>
                <div>
                  <h3>{fiche.manifest?.name ?? fiche.id}</h3>
                  <div className="muted small"><code>{fiche.id}</code>{fiche.manifest ? ` · ${fiche.manifest.version} · ${fiche.manifest.author}` : ""}</div>
                </div>
                <button className="icon-btn" onClick={() => setChoisi(null)} title="Fermer"><I.x size={13} /></button>
              </div>
              <div className="av-traces">
                {fiche.manifest && <BadgeCertification statut={fiche.entree?.manifest.certification ?? fiche.manifest.certification} />}
                {fiche.manifest && <BadgeExecution execution={fiche.manifest.execution} certification={fiche.manifest.certification} />}
                {confiance[fiche.id]?.statut === "quarantaine" && <span className="tag quarantaine">en quarantaine</span>}
                {confiance[fiche.id]?.statut === "retire" && <span className="tag retire">confiance retirée</span>}
              </div>
              <Historique f={fiche} enScene={!!agentsScene[fiche.id]} />
              {fiche.manifest?.description && <p className="av-fiche-desc">{fiche.manifest.description}</p>}
              {!!fiche.manifest?.sait_faire?.length && (
                <div className="av-bloc"><div className="av-titre-liste">Sait faire</div><ul>{fiche.manifest.sait_faire.map((s) => <li key={s}>{s}</li>)}</ul></div>
              )}
              {!!fiche.manifest?.tools.length && (
                <div className="av-bloc">
                  <button className="av-titre-liste av-plier" onClick={() => setFonctions(!fonctions)}><I.chevron size={10} className={fonctions ? "ac-ouvert" : ""} /> Fonctions ({fiche.manifest.tools.length})</button>
                  {fonctions && <div className="ap-tools">{fiche.manifest.tools.map((t) => <span key={t} className={`mini ${t.startsWith("action_sensitive_") ? "sensible" : ""}`}>{t}</span>)}</div>}
                </div>
              )}
              {fiche.manifest?.model && fiche.manifest.model !== "aucun" && <div className="small muted">Modèle : {fiche.manifest.model}</div>}
              {fiche.entree && !fiche.entree.installable && fiche.entree.motif && <div className="ap-motif">{fiche.entree.motif}</div>}
              {fiche.manifest?.avertissement && <div className="ap-avert"><I.alert size={11} /> {fiche.manifest.avertissement}</div>}
              {fiche.statut === "connu" && <div className="ap-motif">Pour qu'Orchestra puisse le choisir, cet agent doit être publié sur l'Agent Market (ou installé depuis un paquet en mode développement).</div>}
              {fiche.installe && fiche.manifest && <ChampsAgent key={fiche.id} agentId={fiche.id} ouvert />}
              <div className="av-fiche-actions">
                {(fiche.statut === "a_installer" || fiche.statut === "maj") && (
                  <button className="glass-btn sm active" disabled={occupe === fiche.id} onClick={() => installer(fiche)}>
                    {occupe === fiche.id ? "vérification…" : fiche.statut === "maj" ? `Mettre à jour (${fiche.installe} → ${fiche.entree?.manifest.version})` : "Installer"}
                  </button>
                )}
                {fiche.installe && fiche.manifest && (
                  <button className="glass-btn sm" onClick={() => { if (agentsScene[fiche.id]) unplug(fiche.id); else { plug(fiche.manifest!); setView("stage"); } }}>
                    {agentsScene[fiche.id] ? <><I.unplug size={11} /> Débrancher de la scène</> : <><I.plug size={11} /> Brancher sur la scène</>}
                  </button>
                )}
                {fiche.installe && <button className="link-btn danger" disabled={occupe === fiche.id} onClick={() => desinstaller(fiche)}>désinstaller</button>}
              </div>
            </motion.aside>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

function PastilleStatut({ f }: { f: Fiche }) {
  const t: Record<Statut, [string, string]> = {
    installe: ["installé", "ok"], maj: ["mise à jour", "maj"], a_installer: ["à installer", "dispo"], bloque: ["non installable", "bloque"], connu: ["hors catalogue", "connu"],
  };
  const [label, cls] = t[f.statut];
  return <span className={`av-pastille ${cls}`}>{label}</span>;
}

/** Ce qu'Orchestra a déjà fait avec l'agent, en badges courts. */
function Traces({ f, enScene }: { f: Fiche; enScene: boolean }) {
  const u = f.usage;
  return (
    <>
      {u && u.missions > 0 && <span className="tag utilise" title={`dernière mission le ${date(u.derniere)}`}>utilisé {u.missions}×</span>}
      {u?.provenance === "market" && <span className="tag" title={`paquet téléchargé depuis l'Agent Market le ${date(u.installe_le)}`}>téléchargé</span>}
      {enScene && <span className="tag scene">sur la scène</span>}
    </>
  );
}

function Historique({ f, enScene }: { f: Fiche; enScene: boolean }) {
  const u = f.usage;
  const lignes: string[] = [];
  if (u && u.missions > 0) lignes.push(`Utilisé ${u.missions} fois (${u.reussies} mission(s) réussie(s)), la dernière le ${date(u.derniere)}.`);
  else if (f.installe) lignes.push("Installé, pas encore utilisé.");
  if (u?.provenance === "market") lignes.push(`Téléchargé depuis l'Agent Market${u.installe_le ? ` le ${date(u.installe_le)}` : ""}.`);
  else if (u?.provenance === "local") lignes.push(`Installé depuis un paquet ou un dossier local${u.installe_le ? ` le ${date(u.installe_le)}` : ""}.`);
  if (enScene) lignes.push("Branché sur la scène en ce moment.");
  if (!lignes.length) return null;
  return <div className="av-historique">{lignes.map((l) => <div key={l}><I.check size={10} /> {l}</div>)}</div>;
}
