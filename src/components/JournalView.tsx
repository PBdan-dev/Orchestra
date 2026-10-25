import { useEffect, useState } from "react";
import { api } from "../tauri";
import { useStore } from "../store";
import { I } from "./Icons";

/** Journal : tout ce qui s'est passé, jour par jour. */
export function JournalView() {
  const [days, setDays] = useState<string[]>([]);
  const [day, setDay] = useState<string>("");
  const [rows, setRows] = useState<any[]>([]);
  const entriesCount = useStore((s) => s.entries.length);

  useEffect(() => {
    api.listJournalDays().then((d) => { setDays(d); if (!day && d[0]) setDay(d[0]); });
  }, [entriesCount]);
  useEffect(() => { if (day) api.readJournal(day).then(setRows); }, [day, entriesCount]);

  return (
    <div className="view journal-view">
      <aside className="file-list">
        <div className="file-list-head"><span>Orchestra/journal</span><button className="icon-btn" onClick={() => api.openFolder("journal")}><I.external size={14} /></button></div>
        {days.length === 0 && <div className="muted small pad">Aucun événement encore.</div>}
        {days.map((d) => <button key={d} className={`file-row ${day === d ? "active" : ""}`} onClick={() => setDay(d)}><I.journal size={13} /> <span className="file-name">{d}</span></button>)}
      </aside>
      <section className="journal-table">
        {rows.length === 0 && <div className="empty"><p className="muted">Rien pour ce jour.</p></div>}
        {rows.slice().reverse().map((r, i) => {
          const ev = r.event ?? {};
          const t = new Date(r.at).toLocaleTimeString("fr-FR");
          return (
            <div key={i} className={`journal-row ${ev.type}`}>
              <span className="t">{t}</span>
              <span className="jtype">{ev.type}</span>
              <span className="jbody">{summary(ev)}</span>
            </div>
          );
        })}
      </section>
    </div>
  );
}

function summary(ev: any): string {
  switch (ev.type) {
    case "phase": return ev.label || ev.phase;
    case "recall": return `${ev.items?.length ?? 0} souvenir(s) rappelé(s)` + (ev.items?.length ? " : " + ev.items.map((i: any) => i.text).join(" · ") : "");
    case "agent_attached": return `${ev.agent?.name} branché depuis ${ev.source} (${ev.reason})`;
    case "agent_status": return `${ev.agent_id} → ${ev.status} ${ev.detail ?? ""}`;
    case "tool_call": return `${ev.agent_id} · ${ev.tool}(${JSON.stringify(ev.input)}) → ${String(ev.output).slice(0, 160)}`;
    case "memorized": return `${ev.file} › ${ev.section} : ${ev.line}`;
    case "nothing_to_memorize": return "rien à retenir";
    case "done": return `[${ev.intent}] ${String(ev.answer).slice(0, 200)}`;
    case "error": return ev.message;
    default: return JSON.stringify(ev).slice(0, 200);
  }
}
