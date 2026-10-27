/**
 * Client du mode vocal : relie le micro et le haut-parleur au serveur voix local
 * (Pipecat, ws://127.0.0.1:<port>), et calcule les niveaux qui font onduler le cœur.
 *
 * - Micro : capté par une AudioWorklet, ramené à 16 kHz mono PCM 16 bits, envoyé par
 *   morceaux de 20 ms. Quand le micro est « fermé » (appuyer-pour-parler relâché, ou
 *   Orchestra qui parle), on envoie du silence : le VAD du serveur voit la fin de phrase.
 * - Voix : PCM 16 bits 24 kHz reçu du serveur, mis bout à bout sans trou.
 * - `levels` est lu par le cœur à chaque image (hors React).
 */

export const levels = {
  mic: 0, // 0..1
  out: 0, // 0..1
  spec: new Float32Array(48), // 0..1 par bande, de la source active
  source: "none" as "none" | "mic" | "out",
};

export type VoiceEvent =
  | { type: "ready"; stt: string; llm: string; tts: string }
  | { type: "user_start" } | { type: "user_stop" }
  | { type: "user_interim"; text: string } | { type: "user_final"; text: string }
  | { type: "bot_start" } | { type: "bot_text"; text: string } | { type: "bot_end" }
  | { type: "delegate"; text: string }
  | { type: "thinking"; on: boolean }
  | { type: "thought"; text: string }
  | { type: "decision"; text: string }
  | { type: "interrupt" }
  | { type: "latency"; total_ms: number; stt_ms: number | null; llm_ms: number | null; tts_ms: number | null; vad_ms: number; [k: string]: unknown }
  | { type: "playback"; playing: boolean }
  | { type: "conn"; state: "off" | "connecting" | "on" | "error"; detail?: string };

const WORKLET = `
class Capture extends AudioWorkletProcessor {
  constructor() { super(); this.ratio = sampleRate / 16000; this.pos = 0; this.buf = new Int16Array(320); this.n = 0; }
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (!ch) return true;
    // décimation simple avec moyenne : sampleRate (48 kHz en général) → 16 kHz
    for (; this.pos < ch.length; this.pos += this.ratio) {
      const i = Math.floor(this.pos);
      const j = Math.min(ch.length - 1, Math.floor(this.pos + this.ratio) - 1);
      let s = 0, k = 0;
      for (let x = i; x <= Math.max(i, j); x++) { s += ch[x]; k++; }
      const v = Math.max(-1, Math.min(1, s / k));
      this.buf[this.n++] = v < 0 ? v * 32768 : v * 32767;
      if (this.n === 320) { this.port.postMessage(this.buf.buffer, [this.buf.buffer]); this.buf = new Int16Array(320); this.n = 0; }
    }
    this.pos -= ch.length;
    return true;
  }
}
registerProcessor("orchestra-capture", Capture);
`;

const OUT_RATE = 24000;

export class VoiceClient {
  private ws: WebSocket | null = null;
  private ctx: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private micAnalyser: AnalyserNode | null = null;
  private node: AudioWorkletNode | null = null;
  private outAnalyser: AnalyserNode | null = null;
  private playHead = 0;
  private sources = new Set<AudioBufferSourceNode>();
  private raf = 0;
  private freq = new Uint8Array(256);
  private time = new Float32Array(1024);
  private wantOpen = false;
  private retry: ReturnType<typeof setTimeout> | null = null;
  private port = 8765;
  /** morceaux de micro envoyés (diagnostic) */
  sent = { voice: 0, silence: 0 };
  /** le micro envoie-t-il le vrai son ? (sinon du silence) */
  gate = true;
  /** couper le micro pendant que la voix joue (évite qu'Orchestra s'entende elle-même) */
  muteWhileSpeaking = true;
  playing = false;

  constructor(private onEvent: (e: VoiceEvent) => void) {}

  get connected() { return this.ws?.readyState === WebSocket.OPEN; }

  async connect(port: number) {
    this.wantOpen = true;
    this.port = port;
    if (this.retry) { clearTimeout(this.retry); this.retry = null; }
    // une seule connexion à la fois : le serveur refuse un second client
    if (this.ws && this.ws.readyState <= WebSocket.OPEN) return;
    this.onEvent({ type: "conn", state: "connecting" });
    await this.ensureAudio();
    if (this.ws && this.ws.readyState <= WebSocket.OPEN) return;
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => {
      if (this.ws !== ws) { ws.close(); return; }
      this.onEvent({ type: "conn", state: "on" });
    };
    ws.onclose = () => {
      if (this.ws !== ws) return; // une ancienne connexion : on l'ignore
      this.ws = null;
      this.flushPlayback();
      this.onEvent({ type: "conn", state: "off" });
      // le serveur charge ses modèles (≈30 s au démarrage) : on réessaie tant qu'on le veut
      if (this.wantOpen && !this.retry) this.retry = setTimeout(() => { this.retry = null; if (this.wantOpen) this.connect(this.port); }, 1500);
    };
    ws.onerror = () => { if (this.ws === ws) this.onEvent({ type: "conn", state: "error", detail: "serveur voix injoignable" }); };
    ws.onmessage = (m) => {
      if (this.ws !== ws) return;
      if (typeof m.data === "string") {
        let e: VoiceEvent;
        try { e = JSON.parse(m.data); } catch { return; }
        if (e.type === "interrupt") this.flushPlayback();
        if (e.type === "ready") this.onEvent({ type: "conn", state: "on" });
        this.onEvent(e);
      } else {
        this.play(m.data as ArrayBuffer);
      }
    };
  }

  disconnect() {
    this.wantOpen = false;
    if (this.retry) { clearTimeout(this.retry); this.retry = null; }
    this.ws?.close();
    this.ws = null;
    this.stopMic();
    this.flushPlayback();
  }

  private async ensureAudio() {
    if (!this.ctx) {
      this.ctx = new AudioContext({ latencyHint: "interactive" });
      this.outAnalyser = this.ctx.createAnalyser();
      this.outAnalyser.fftSize = 512;
      this.outAnalyser.smoothingTimeConstant = 0.6;
      this.outAnalyser.connect(this.ctx.destination);
      const url = URL.createObjectURL(new Blob([WORKLET], { type: "application/javascript" }));
      await this.ctx.audioWorklet.addModule(url);
      this.loop();
    }
    if (this.ctx.state === "suspended") {
      await Promise.race([this.ctx.resume(), new Promise((r) => setTimeout(r, 1500))]);
    }
  }

  /** Ouvre le micro (une seule fois ; ensuite `gate` décide si le son part). */
  async startMic() {
    await this.ensureAudio();
    if (this.stream) return;
    const ctx = this.ctx!;
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
    });
    const src = ctx.createMediaStreamSource(this.stream);
    this.micAnalyser = ctx.createAnalyser();
    this.micAnalyser.fftSize = 512;
    this.micAnalyser.smoothingTimeConstant = 0.55;
    src.connect(this.micAnalyser);
    const node = new AudioWorkletNode(ctx, "orchestra-capture");
    this.node = node;
    node.port.onmessage = (m) => {
      if (!this.connected) return;
      const open = this.gate && !(this.muteWhileSpeaking && this.playing);
      this.ws!.send(open ? (m.data as ArrayBuffer) : new ArrayBuffer(640));
      if (open) this.sent.voice++; else this.sent.silence++;
    };
    src.connect(node);
    // la worklet doit être reliée à la sortie pour tourner ; gain nul = muet
    const sink = ctx.createGain();
    sink.gain.value = 0;
    node.connect(sink).connect(ctx.destination);
  }

  stopMic() {
    this.node?.port.close();
    this.node?.disconnect();
    this.node = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.micAnalyser = null;
  }

  get micOpen() { return !!this.stream; }

  send(msg: object) {
    if (this.connected) this.ws!.send(JSON.stringify(msg));
  }

  /** Message tapé pendant le mode voix : même cerveau rapide, réponse dite. */
  sendText(text: string) {
    this.send({ type: "user_text", text });
  }

  /** Lit un texte avec la voix d'Orchestra. */
  say(text: string) {
    const t = speakable(text);
    if (t) this.send({ type: "say", text: t });
  }

  /** Coupe la parole d'Orchestra. */
  stopSpeaking() {
    this.flushPlayback();
    this.send({ type: "stop" });
  }

  private play(buf: ArrayBuffer) {
    const ctx = this.ctx;
    if (!ctx || !this.outAnalyser) return;
    const pcm = new Int16Array(buf);
    if (!pcm.length) return;
    const ab = ctx.createBuffer(1, pcm.length, OUT_RATE);
    const ch = ab.getChannelData(0);
    for (let i = 0; i < pcm.length; i++) ch[i] = pcm[i] / 32768;
    const src = ctx.createBufferSource();
    src.buffer = ab;
    src.connect(this.outAnalyser);
    const at = Math.max(ctx.currentTime + 0.03, this.playHead);
    src.start(at);
    this.playHead = at + ab.duration;
    this.sources.add(src);
    src.onended = () => {
      this.sources.delete(src);
      if (this.sources.size === 0) this.setPlaying(false);
    };
    this.setPlaying(true);
  }

  private flushPlayback() {
    for (const s of this.sources) { try { s.stop(); } catch { /* déjà fini */ } }
    this.sources.clear();
    this.playHead = 0;
    this.setPlaying(false);
  }

  private setPlaying(p: boolean) {
    if (p === this.playing) return;
    this.playing = p;
    this.onEvent({ type: "playback", playing: p });
  }

  /** Niveaux et spectre, ~60 fois par seconde. */
  private loop = () => {
    if (this.playing && this.ctx && this.ctx.currentTime > this.playHead + 0.4) {
      this.sources.clear();
      this.setPlaying(false);
    }
    const read = (a: AnalyserNode | null) => {
      if (!a) return 0;
      a.getFloatTimeDomainData(this.time.subarray(0, a.fftSize));
      let s = 0;
      for (let i = 0; i < a.fftSize; i++) s += this.time[i] * this.time[i];
      return Math.min(1, Math.sqrt(s / a.fftSize) * 4.5);
    };
    const micOn = !!this.micAnalyser && this.gate && !(this.muteWhileSpeaking && this.playing);
    const mic = micOn ? read(this.micAnalyser) : 0;
    const out = this.playing ? read(this.outAnalyser) : 0;
    levels.mic += (mic - levels.mic) * 0.35;
    levels.out += (out - levels.out) * 0.35;
    const a = this.playing ? this.outAnalyser : micOn ? this.micAnalyser : null;
    levels.source = this.playing ? "out" : micOn ? "mic" : "none";
    if (a) {
      a.getByteFrequencyData(this.freq.subarray(0, a.frequencyBinCount));
      // bandes de la voix (≈80 Hz à 6 kHz), réparties de façon logarithmique
      const n = levels.spec.length, bins = a.frequencyBinCount, nyq = (this.ctx?.sampleRate ?? 48000) / 2;
      for (let b = 0; b < n; b++) {
        const f0 = 80 * Math.pow(6000 / 80, b / n), f1 = 80 * Math.pow(6000 / 80, (b + 1) / n);
        const i0 = Math.floor((f0 / nyq) * bins), i1 = Math.max(i0 + 1, Math.floor((f1 / nyq) * bins));
        let m = 0;
        for (let i = i0; i < i1 && i < bins; i++) m = Math.max(m, this.freq[i]);
        levels.spec[b] += (m / 255 - levels.spec[b]) * 0.4;
      }
    } else {
      for (let b = 0; b < levels.spec.length; b++) levels.spec[b] *= 0.85;
    }
    this.raf = requestAnimationFrame(this.loop);
  };
}

/** Texte prêt à être dit : sans Markdown, sans code, pas trop long. */
export function speakable(md: string, max = 600): string {
  let t = md
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^#+\s*/gm, "")
    .replace(/^\s*[-*•]\s+/gm, "")
    .replace(/[*_~>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (t.length > max) {
    const cut = t.slice(0, max);
    const end = Math.max(cut.lastIndexOf(". "), cut.lastIndexOf("! "), cut.lastIndexOf("? "));
    t = (end > 120 ? cut.slice(0, end + 1) : cut) + " Le reste est affiché à l'écran.";
  }
  return t;
}
