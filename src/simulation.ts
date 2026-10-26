/**
 * Simulation de branchements : rejoue une demande fictive où Orchestra branche trois agents,
 * dont un qui délègue à son tour à un sous-agent. Tout passe par le même circuit d'événements
 * que le vrai moteur, mais rien n'est écrit sur le disque et rien n'est envoyé au modèle.
 */
import { live, uid, useStore } from "./store";
import type { Manifest, OrchestraEvent } from "./types";

const R = "sim";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const agent = (p: Partial<Manifest> & Pick<Manifest, "id" | "name">): Manifest => ({
  version: "0.0.1", author: "simulation", description: "", capabilities: [], model: "inherit",
  system_prompt: "", tools: [], color: "#818cf8", icon: "spark", ...p,
});

const MAIL = agent({
  id: "sim-mail", name: "Agent Mail", color: "#f472b6", icon: "mail",
  description: "Lit, trie et résume les mails de la boîte de réception.",
  capabilities: ["mails", "tri"], tools: ["mail.list", "mail.read", "delegate"],
  system_prompt: "Tu es l'Agent Mail. Tu tries la boîte de réception par importance et tu délègues les résumés longs à un sous-agent.",
});
const RESUME = agent({
  id: "sim-resume", name: "Sous-agent Résumé", color: "#c084fc", icon: "pen",
  description: "Sous-agent créé par l'Agent Mail pour condenser les fils de discussion longs.",
  capabilities: ["résumé"], tools: ["summarize"],
  system_prompt: "Tu résumes un fil de mails en trois puces : décision, action attendue, échéance.",
});
const AGENDA = agent({
  id: "sim-agenda", name: "Agent Agenda", color: "#fbbf24", icon: "calendar",
  description: "Trouve des créneaux libres et crée des événements.",
  capabilities: ["agenda", "réunion"], tools: ["calendar.free_slots", "calendar.create"],
  system_prompt: "Tu es l'Agent Agenda. Tu proposes des créneaux sans chevauchement et tu crées les réunions demandées.",
});
const VEILLE = agent({
  id: "sim-veille", name: "Agent Veille Web", color: "#60a5fa", icon: "globe",
  description: "Lit des pages web et en fait une synthèse sourcée.",
  capabilities: ["web", "veille"], tools: ["fetch_url"],
  system_prompt: "Tu lis les pages demandées et tu en fais une synthèse courte avec la source.",
});

export async function runSimulation() {
  const st = useStore.getState();
  if (st.busy || st.simulating) return;
  const emit = (e: OrchestraEvent) => useStore.getState().handle(e);
  const think = async (text: string, speed = 14) => {
    for (const w of text.split(/(\s+)/)) { emit({ type: "thought", run_id: R, text: w }); await sleep(speed); }
  };
  const say = async (text: string) => {
    for (const w of text.split(/(\s+)/)) { emit({ type: "token", run_id: R, text: w }); await sleep(24); }
  };
  const phase = (p: any, label: string) => emit({ type: "phase", run_id: R, phase: p, label });
  const tool = async (a: Manifest, name: string, input: unknown, output: string, wait = 900) => {
    emit({ type: "tool_call", run_id: R, agent_id: a.id, tool: name, input, output });
    await sleep(wait);
  };

  const ask = "Trie mes mails de la semaine, résume les importants, cale les réunions demandées et fais-moi une veille sur Tauri 2.";
  useStore.setState((s) => ({
    simulating: true,
    busy: true,
    messages: [
      ...s.messages,
      { id: uid(), role: "user", content: `${ask}`, at: Date.now(), sim: true },
      { id: uid(), role: "assistant", content: "", streaming: true, at: Date.now(), sim: true },
    ],
    entries: [...s.entries, { kind: "user", id: uid(), runId: R, at: Date.now(), text: `[simulation] ${ask}` }],
  }));
  live.pulse(0.7);

  try {
    phase("recalling", "Je consulte ma mémoire");
    await sleep(600);
    emit({ type: "recall", run_id: R, items: [
      { file: "orchestra.md", section: "Agents de confiance", text: "« trier des mails » → sim-mail (Agent Mail) · réussi 3×", score: 2.1, reason: "mots communs : mails, trier (exemple fictif)" },
      { file: "orchestra.md", section: "Préférences", text: "PB préfère les réunions l'après-midi.", score: 1.2, reason: "mots communs : réunions (exemple fictif)" },
    ] });
    await sleep(500);

    phase("thinking", "J'analyse la demande");
    await think("La demande contient trois travaux distincts : le tri des mails, la planification de réunions, et une veille web. Je ne les fais pas moi-même : je branche un agent par travail, et ils travaillent en parallèle. Ma mémoire indique que l'Agent Mail a déjà réussi ce type de tâche.\n→ Intention : travail · 3 agents nécessaires\n");

    phase("searching_agent", "Je cherche les agents qu'il faut");
    await think("🧠 Mémoire : « trier des mails » → Agent Mail (réussi 3×)\n🔎 Agent Market : Agent Agenda (score 4.0), Agent Veille Web (score 3.5)\n");
    await sleep(300);

    // 1. Agent Mail
    emit({ type: "agent_attached", run_id: R, agent: MAIL, source: "mémoire", reason: "agent de confiance pour « trier des mails »" });
    phase("delegating", "Je confie les missions");
    emit({ type: "agent_status", run_id: R, agent_id: MAIL.id, status: "running", detail: "Trier les mails de la semaine et résumer les importants" });
    await sleep(900);

    // 2. Agent Agenda
    emit({ type: "agent_attached", run_id: R, agent: AGENDA, source: "Agent Market", reason: "meilleur score (4.0) pour « caler des réunions »" });
    emit({ type: "agent_status", run_id: R, agent_id: AGENDA.id, status: "running", detail: "Caler les réunions demandées dans les mails" });
    await sleep(900);

    // 3. Agent Veille
    emit({ type: "agent_attached", run_id: R, agent: VEILLE, source: "Agent Market", reason: "meilleur score (3.5) pour « veille web »" });
    emit({ type: "agent_status", run_id: R, agent_id: VEILLE.id, status: "running", detail: "Veille sur Tauri 2 : nouveautés et points d'attention" });
    phase("working", "3 agents travaillent");
    await sleep(700);

    await tool(MAIL, "mail.list", { since: "lundi" }, "47 mails · 6 marqués importants");
    await tool(VEILLE, "fetch_url", { url: "https://v2.tauri.app/blog/" }, "Page lue : 18 400 caractères");
    await tool(MAIL, "mail.read", { ids: [3, 8, 12, 19, 27, 41] }, "6 mails lus, dont 2 fils de plus de 20 messages");

    // sous-agent
    await think("[Agent Mail] Deux fils sont trop longs : je délègue leur résumé à un sous-agent.\n");
    emit({ type: "agent_attached", run_id: R, agent: RESUME, source: "créé par Agent Mail", reason: "fils de discussion trop longs", parent: MAIL.id });
    emit({ type: "agent_status", run_id: R, agent_id: RESUME.id, status: "running", detail: "Résumer 2 fils (devis Delta, lancement V2)" });
    await sleep(800);
    await tool(AGENDA, "calendar.free_slots", { week: "prochaine", après: "14:00" }, "5 créneaux libres l'après-midi");
    await tool(RESUME, "summarize", { thread: "Devis Delta" }, "Décision : accepter v3 · Action : signer · Échéance : jeudi");
    await tool(RESUME, "summarize", { thread: "Lancement V2" }, "Décision : report au 12 · Action : prévenir l'équipe");
    emit({ type: "agent_status", run_id: R, agent_id: RESUME.id, status: "done", detail: "2 fils résumés" });
    await tool(AGENDA, "calendar.create", { titre: "Point Delta", quand: "mardi 15:00" }, "Réunion créée");
    emit({ type: "agent_status", run_id: R, agent_id: VEILLE.id, status: "done", detail: "Synthèse en 5 points" });
    await sleep(400);
    await tool(AGENDA, "calendar.create", { titre: "Revue lancement", quand: "jeudi 16:00" }, "Réunion créée");
    emit({ type: "agent_status", run_id: R, agent_id: AGENDA.id, status: "done", detail: "2 réunions calées l'après-midi" });
    emit({ type: "agent_status", run_id: R, agent_id: MAIL.id, status: "done", detail: "6 mails importants triés et résumés" });

    phase("speaking", "Je restitue les résultats");
    await say("**Simulation** — voici ce que les trois agents ont rendu :\n\n- **Agent Mail** a trié 47 mails : 6 importants. Il a confié deux fils longs à un **sous-agent Résumé** (devis Delta à signer jeudi, lancement V2 reporté au 12).\n- **Agent Agenda** a calé 2 réunions l'après-midi, comme tu préfères : mardi 15 h et jeudi 16 h.\n- **Agent Veille Web** a lu le blog de Tauri et prépare une synthèse en 5 points.\n\nRien de tout cela n'est réel : c'est une démonstration des branchements. Débranche un agent d'un clic, les autres continuent de fonctionner.");

    phase("memorizing", "Je retiens l'essentiel");
    await sleep(700);
    emit({ type: "memorized", run_id: R, file: "orchestra.md (simulation, non écrit)", section: "Agents de confiance", line: "- « caler des réunions » → sim-agenda (Agent Agenda) · réussi 1×" });
    emit({ type: "memorized", run_id: R, file: "agents/sim-mail.md (simulation, non écrit)", section: "Missions", line: "- Mission : trier les mails → 6 importants · outils : mail.list, mail.read, delegate" });
    emit({ type: "done", run_id: R, answer: "", intent: "travail (simulation)" });
    phase("idle", "");
  } finally {
    useStore.setState({ simulating: false, busy: false });
  }
}
