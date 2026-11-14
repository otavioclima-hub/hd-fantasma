import type { Item } from "./types";

export function formatBytes(n: number, locale = "pt-BR"): string {
  if (!isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log10(n) / 3));
  const v = n / Math.pow(1000, i);
  const nf = new Intl.NumberFormat(locale, { maximumFractionDigits: i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0 });
  return `${nf.format(v)} ${units[i]}`;
}

export function formatNumber(n: number, locale = "pt-BR"): string {
  return new Intl.NumberFormat(locale).format(n);
}

export function formatDate(iso: string | null, locale = "pt-BR", withTime = true): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "—";
  return new Intl.DateTimeFormat(locale, withTime ? { dateStyle: "short", timeStyle: "short" } : { dateStyle: "medium" }).format(d);
}

export function parentPath(item: Item): string {
  const i = item.path.lastIndexOf("/");
  return i < 0 ? "" : item.path.slice(0, i);
}

export function debounce<T extends unknown[]>(fn: (...a: T) => void, ms: number) {
  let h: ReturnType<typeof setTimeout> | undefined;
  return (...a: T) => {
    clearTimeout(h);
    h = setTimeout(() => fn(...a), ms);
  };
}
