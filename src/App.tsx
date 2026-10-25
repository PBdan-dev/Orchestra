import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "./store";
import { TopBar } from "./components/TopBar";
import { Stage } from "./components/Stage";
import { Conscience } from "./components/Conscience";
import { Conversation } from "./components/Conversation";
import { Composer } from "./components/Composer";
import { MemoryView } from "./components/MemoryView";
import { SettingsView } from "./components/SettingsView";
import { JournalView } from "./components/JournalView";

export default function App() {
  const boot = useStore((s) => s.boot);
  const bootDone = useStore((s) => s.bootDone);
  const view = useStore((s) => s.view);
  const [minDelay, setMinDelay] = useState(false);

  useEffect(() => {
    boot().catch((e) => console.error(e));
    const t = setTimeout(() => setMinDelay(true), 1400);
    return () => clearTimeout(t);
  }, []);

  const ready = bootDone && minDelay;

  return (
    <div className="app">
      <AnimatePresence>
        {!ready && (
          <motion.div className="splash" exit={{ opacity: 0, scale: 1.04 }} transition={{ duration: 0.6 }}>
            <motion.div className="splash-ring" initial={{ scale: 0.6, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} transition={{ duration: 0.9, ease: "easeOut" }} />
            <motion.div className="splash-ring r2" initial={{ scale: 0.4, opacity: 0 }} animate={{ scale: 1, opacity: 0.5 }} transition={{ duration: 1.2, ease: "easeOut" }} />
            <motion.h1 initial={{ opacity: 0, letterSpacing: "0.6em" }} animate={{ opacity: 1, letterSpacing: "0.28em" }} transition={{ duration: 1.1, delay: 0.2 }}>ORCHESTRA</motion.h1>
            <motion.p initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ delay: 0.8 }}>un orchestrateur, en toute transparence</motion.p>
          </motion.div>
        )}
      </AnimatePresence>

      <TopBar />
      <main className="main">
        <AnimatePresence mode="wait">
          {view === "stage" && (
            <motion.div key="stage" className="layout-stage" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}>
              <Conscience />
              <Stage />
              <Conversation />
            </motion.div>
          )}
          {view === "memory" && <motion.div key="memory" className="layout-view" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}><MemoryView /></motion.div>}
          {view === "journal" && <motion.div key="journal" className="layout-view" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}><JournalView /></motion.div>}
          {view === "settings" && <motion.div key="settings" className="layout-view" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}><SettingsView /></motion.div>}
        </AnimatePresence>
      </main>
      {view === "stage" && <Composer />}
    </div>
  );
}
