// Tempo restante pela velocidade dos últimos ~20 s (janela deslizante).
// Usado pela tela de progresso da Leitura (docs/CONTRATO.md).

export const WINDOW_MS = 20_000;
export const WARMUP_MS = 5_000;
/** Velocidade abaixo disso (unidades por segundo) conta como "parado". */
const MIN_RATE = 1;

export type EtaResult =
  | { state: "calculating" }
  | { state: "unknown" } // sem velocidade confiável (ex.: pastas da nuvem): mostrar "—"
  | { state: "ok"; seconds: number };

export class Eta {
  private samples: { t: number; v: number }[] = [];
  private startT: number | null = null;

  reset() {
    this.samples = [];
    this.startT = null;
  }

  /** `t` em milissegundos (relógio qualquer, crescente), `v` = quanto já foi feito. */
  add(t: number, v: number) {
    if (this.startT === null) this.startT = t;
    this.samples.push({ t, v });
    const min = t - WINDOW_MS;
    while (this.samples.length > 2 && this.samples[1].t < min) this.samples.shift();
  }

  /** Velocidade (unidades/s) na janela, ou 0. */
  rate(): number {
    if (this.samples.length < 2) return 0;
    const a = this.samples[0], b = this.samples[this.samples.length - 1];
    const dt = (b.t - a.t) / 1000;
    return dt > 0 ? Math.max(0, (b.v - a.v) / dt) : 0;
  }

  /**
   * `remaining` = quanto falta. `stalledIsUnknown`: com velocidade ~0, devolve "unknown"
   * em vez de "calculating" (nuvem: itens crescem, bytes não).
   */
  estimate(now: number, remaining: number, stalledIsUnknown = false): EtaResult {
    if (this.startT === null || now - this.startT < WARMUP_MS) return { state: "calculating" };
    const r = this.rate();
    if (r < MIN_RATE) return { state: stalledIsUnknown ? "unknown" : "calculating" };
    return { state: "ok", seconds: Math.max(0, remaining) / r };
  }
}

/** Segundos -> { kind: "less" } (< 1 min) ou { kind: "t", text: "12 min" | "2 h 10 min" }. */
export function formatRemaining(sec: number): { kind: "less" } | { kind: "t"; text: string } {
  const min = Math.ceil(sec / 60);
  if (sec < 60) return { kind: "less" };
  if (min < 60) return { kind: "t", text: `${min} min` };
  const h = Math.floor(min / 60), m = min % 60;
  return { kind: "t", text: m ? `${h} h ${m} min` : `${h} h` };
}

/** Meio do caminho cortado: "Projetos/2025/…/arquivo.mov". */
export function truncateMiddle(s: string, max = 70): string {
  if (s.length <= max) return s;
  const keepEnd = Math.ceil((max - 1) * 0.6);
  const keepStart = max - 1 - keepEnd;
  return `${s.slice(0, keepStart)}…${s.slice(s.length - keepEnd)}`;
}
