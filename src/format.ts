/* Locale-aware formatting. The backend now sends raw numbers; every unit and
   separator is decided here, so switching language actually switches units
   (it used to send French "45.2 Mo" strings that the English UI showed as-is). */
import type { Lang } from "./i18n";

const LOCALE: Record<Lang, string> = { fr: "fr-FR", en: "en-US" };

// Decimal (SI) units, matching what servers and download sites quote.
const UNITS: Record<Lang, string[]> = {
  fr: ["o", "ko", "Mo", "Go", "To"],
  en: ["B", "KB", "MB", "GB", "TB"],
};

/** "412 Mo" / "1,24 Go". Empty string when the size is unknown. */
export function bytes(n: number, lang: Lang): string {
  if (!n || n <= 0 || !Number.isFinite(n)) return "";
  const units = UNITS[lang];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  // More precision the smaller the number reads, so "1,24 Go" doesn't collapse
  // to a "1 Go" that hides a 240 MB difference.
  const digits = i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0;
  return `${v.toLocaleString(LOCALE[lang], {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })} ${units[i]}`;
}

/** "412 Mo / 1,24 Go", or just the total, or just what's downloaded. */
export function progressBytes(done: number, total: number, lang: Lang): string {
  const d = bytes(done, lang);
  const t = bytes(total, lang);
  if (d && t) return `${d} / ${t}`;
  return t || d;
}

/** A duration in seconds: "4:12", "1:02:33". Empty when unknown. */
export function duration(secs: number): string {
  if (!secs || secs <= 0) return "";
  const s = Math.round(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** "il y a 3 min" / "3 min ago". Empty for a 0 timestamp (unknown). */
export function since(unixSecs: number, lang: Lang): string {
  if (!unixSecs) return "";
  const diff = Math.round(unixSecs - Date.now() / 1000);
  const rtf = new Intl.RelativeTimeFormat(LOCALE[lang], { numeric: "auto" });
  const steps: [number, Intl.RelativeTimeFormatUnit][] = [
    [60, "second"],
    [3600, "minute"],
    [86400, "hour"],
    [604800, "day"],
  ];
  for (const [limit, unit] of steps) {
    if (Math.abs(diff) < limit) {
      const div = limit === 60 ? 1 : limit === 3600 ? 60 : limit === 86400 ? 3600 : 86400;
      return rtf.format(Math.trunc(diff / div), unit);
    }
  }
  return new Date(unixSecs * 1000).toLocaleDateString(LOCALE[lang], {
    day: "numeric",
    month: "short",
  });
}

/** Percentage with one decimal below 100 — matches the old card exactly. */
export function percent(p: number, lang: Lang): string {
  const v = Math.min(Math.max(p, 0), 100);
  return `${v.toLocaleString(LOCALE[lang], {
    minimumFractionDigits: v < 100 ? 1 : 0,
    maximumFractionDigits: v < 100 ? 1 : 0,
  })} %`;
}

/** "4,2 Mo/s". Empty when idle. */
export function speed(bps: number, lang: Lang): string {
  const b = bytes(bps, lang);
  return b ? `${b}/s` : "";
}

/** Remaining time: "00:42", "1:02:33". Empty when unknown (-1). */
export function eta(secs: number): string {
  return secs < 0 ? "" : duration(Math.max(secs, 1));
}

/** Host of a link, for the second line of a card. */
export function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}
