import { useEffect, useRef } from "react";
import { live } from "../store";
import { PHASE_COLOR, type Phase } from "../types";

/**
 * Le cœur : un cercle dont les ondes et les bandes réagissent à ce qu'Orchestra fait.
 * Dessiné sur canvas à 60 i/s ; l'énergie (frappe, fragments reçus, événements) vient de `live`.
 */
export function Core({ phase, cx, cy, R }: { phase: Phase; cx: number; cy: number; R: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const phaseRef = useRef(phase);
  phaseRef.current = phase;
  const geo = useRef({ cx, cy, R });
  geo.current = { cx, cy, R };

  useEffect(() => {
    const canvas = ref.current!;
    const ctx = canvas.getContext("2d")!;
    let raf = 0;
    let w = 0, h = 0, dpr = 1;
    const col = { r: 124, g: 140, b: 248 };
    let rot = 0;
    let smoothAmp = 3;
    let smoothTokens = 0;
    const start = performance.now();

    const resize = () => {
      const parent = canvas.parentElement!;
      dpr = Math.min(2, window.devicePixelRatio || 1);
      w = parent.clientWidth; h = parent.clientHeight;
      canvas.width = w * dpr; canvas.height = h * dpr;
      canvas.style.width = `${w}px`; canvas.style.height = `${h}px`;
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas.parentElement!);

    const hex = (c: string) => ({ r: parseInt(c.slice(1, 3), 16), g: parseInt(c.slice(3, 5), 16), b: parseInt(c.slice(5, 7), 16) });

    const frame = (now: number) => {
      const t = (now - start) / 1000;
      const p = phaseRef.current;
      const { cx, cy, R } = geo.current;

      // énergie et tokens décroissent
      live.energy *= 0.955;
      live.tokens *= 0.93;
      smoothTokens += (live.tokens - smoothTokens) * 0.15;
      const typing = Date.now() - live.lastKey < 400 ? 1 : 0;

      const base: Record<Phase, number> = {
        idle: 2.2, listening: 4 + typing * 5, thinking: 9, recalling: 6, searching_agent: 7,
        delegating: 8, working: 10, speaking: 5 + Math.min(8, smoothTokens * 0.6), memorizing: 5, error: 6,
      };
      const targetAmp = base[p] * (0.65 + live.energy * 0.9);
      smoothAmp += (targetAmp - smoothAmp) * 0.08;
      const amp = smoothAmp;

      const target = hex(PHASE_COLOR[p]);
      col.r += (target.r - col.r) * 0.05; col.g += (target.g - col.g) * 0.05; col.b += (target.b - col.b) * 0.05;
      const c = `${col.r | 0},${col.g | 0},${col.b | 0}`;

      const speed = p === "thinking" || p === "working" ? 1.6 : p === "idle" ? 0.35 : 1;
      rot += 0.004 * speed;

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      // halo
      const glow = ctx.createRadialGradient(cx, cy, R * 0.4, cx, cy, R * 2.3);
      glow.addColorStop(0, `rgba(${c},${0.16 + live.energy * 0.18})`);
      glow.addColorStop(0.5, `rgba(${c},0.05)`);
      glow.addColorStop(1, "rgba(0,0,0,0)");
      ctx.fillStyle = glow;
      ctx.fillRect(0, 0, w, h);

      // ondes de rappel (recalling) : cercles qui convergent ; mémorisation : qui se contractent et s'éteignent
      if (p === "recalling" || p === "memorizing" || p === "searching_agent") {
        for (let k = 0; k < 3; k++) {
          const ph = ((t * 0.55 + k / 3) % 1);
          const rr = p === "searching_agent" ? R + 20 + ph * 120 : R + 20 + (1 - ph) * 120;
          ctx.beginPath();
          ctx.arc(cx, cy, rr, 0, Math.PI * 2);
          ctx.strokeStyle = `rgba(${c},${(p === "searching_agent" ? 1 - ph : ph) * 0.25})`;
          ctx.lineWidth = 1;
          ctx.stroke();
        }
      }

      // anneaux d'ondes
      const N = 220;
      for (let k = 0; k < 3; k++) {
        ctx.beginPath();
        for (let i = 0; i <= N; i++) {
          const a = (i / N) * Math.PI * 2;
          const f1 = 5 + k, f2 = 9 + k * 2, f3 = 3;
          const n =
            Math.sin(a * f1 + t * (1.3 + k * 0.3) * speed + rot * 3) * 0.55 +
            Math.sin(a * f2 - t * (0.9 + k * 0.2) * speed) * 0.3 +
            Math.sin(a * f3 + t * 0.4) * 0.15;
          const r = R + 6 + k * 7 + n * amp * (1 + k * 0.35);
          const x = cx + Math.cos(a + rot) * r, y = cy + Math.sin(a + rot) * r;
          i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
        }
        ctx.closePath();
        ctx.strokeStyle = `rgba(${c},${[0.85, 0.42, 0.2][k]})`;
        ctx.lineWidth = [1.8, 1.2, 0.9][k];
        ctx.shadowColor = `rgba(${c},0.6)`;
        ctx.shadowBlur = k === 0 ? 14 : 0;
        ctx.stroke();
        ctx.shadowBlur = 0;
      }

      // disque central
      const disc = ctx.createRadialGradient(cx - R * 0.3, cy - R * 0.35, R * 0.1, cx, cy, R);
      disc.addColorStop(0, "rgba(28,32,54,0.98)");
      disc.addColorStop(1, "rgba(9,10,18,0.98)");
      ctx.beginPath();
      ctx.arc(cx, cy, R, 0, Math.PI * 2);
      ctx.fillStyle = disc;
      ctx.fill();
      ctx.strokeStyle = `rgba(${c},0.35)`;
      ctx.lineWidth = 1;
      ctx.stroke();

      // bandes sonores circulaires
      const bars = 84;
      const inner = R - 30;
      for (let i = 0; i < bars; i++) {
        const a = (i / bars) * Math.PI * 2 - rot * 0.6;
        const noise = Math.abs(Math.sin(i * 0.7 + t * 3.1 * speed) * Math.sin(i * 0.31 - t * 1.7 * speed) * Math.sin(i * 1.3 + t * 0.9));
        const idleFactor = p === "idle" ? 0.45 : 1;
        const hgt = 2 + noise * amp * 1.7 * idleFactor + typing * 4;
        const x0 = cx + Math.cos(a) * inner, y0 = cy + Math.sin(a) * inner;
        const x1 = cx + Math.cos(a) * (inner + hgt), y1 = cy + Math.sin(a) * (inner + hgt);
        ctx.beginPath();
        ctx.moveTo(x0, y0); ctx.lineTo(x1, y1);
        ctx.strokeStyle = `rgba(${c},${0.25 + noise * 0.6})`;
        ctx.lineWidth = 2;
        ctx.lineCap = "round";
        ctx.stroke();
      }

      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); };
  }, []);

  return <canvas ref={ref} className="core-canvas" />;
}
