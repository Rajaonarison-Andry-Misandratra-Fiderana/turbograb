/** Mirrors `DownloadInfo` in src-tauri/src/lib.rs. Keep the two in step. */

export type Status =
  /** Added, waiting for a free slot — see `max_active` in settings. */
  | "queued"
  | "downloading"
  | "paused"
  | "interrupted"
  | "done"
  | "error";

/** Where the item came from: pasted into the app, or handed over by the
 *  browser extension. Shown on the card so a download that appeared on its own
 *  is explained rather than mysterious. */
export type Source = "app" | "browser";

export interface DownloadInfo {
  id: string;
  url: string;
  out_dir: string;
  title: string;
  percent: number;
  /** Always an "@code" — the backend never sends raw failure text. */
  error_msg: string;
  /** The raw diagnostic behind it. Never rendered; copyable on demand. */
  error_detail: string;
  status: Status;

  total_bytes: number;
  downloaded_bytes: number;
  connections: number;
  resumable: boolean;
  /** Raw bytes/s; 0 = idle. Formatted in the UI so units follow the language. */
  speed_bps: number;
  /** Seconds remaining; -1 = unknown. */
  eta_secs: number;
  /** Current attempt while recovering a dropped connection; 0 = not retrying. */
  retry: number;
  retry_max: number;
  /** Completion per bucket (0-100) across the parallel connections. Live only:
   *  absent on a freshly loaded item, and never written to disk. */
  segments: number[];
  created_at: number;
  file_path: string;
  /** Unix seconds; 0 = unknown. */
  started_at: number;
  finished_at: number;
  /** Request headers replayed on every connection — cookies, referer, the
   *  browser's user-agent. Empty for a link pasted into the app. */
  headers: Record<string, string>;
  source: Source;
}

/** Mirrors `Settings` in src-tauri/src/lib.rs. */
export interface Settings {
  out_dir: string;
  lang: string;
  theme: string;
  tray_hint_shown: boolean;
  connections: number;
  max_active: number;
  autostart: boolean;
  start_hidden: boolean;
  server_enabled: boolean;
  server_port: number;
  /** Read-only here: only pairing mints it. */
  token: string;
}

/** Backend flags a 401/403/410 on a link as `@link_expired`: the bytes on disk
 *  are still good, only the address went stale. */
export const isExpired = (d: DownloadInfo) =>
  d.status === "error" && d.error_msg === "@link_expired";

export const isFinished = (d: DownloadInfo) =>
  d.status === "done" || d.status === "error";

export const isRunning = (d: DownloadInfo) =>
  d.status === "downloading" || d.status === "queued";

/** Newest first, and stable: sorting by status made cards jump mid-download. */
export const byNewest = (a: DownloadInfo, b: DownloadInfo) =>
  b.created_at - a.created_at || a.id.localeCompare(b.id);

/** Split whatever was pasted into links.
 *
 *  A copied block of links is one paste, not one download: newlines, spaces and
 *  tabs all separate. Anything that isn't an http(s) URL is dropped rather than
 *  queued as a card that can only fail. */
export function parseLinks(text: string): string[] {
  const seen = new Set<string>();
  return text
    .split(/[\s\r\n]+/)
    .map((s) => s.trim().replace(/[),.;]+$/, ""))
    .filter((s) => /^https?:\/\/\S+$/i.test(s))
    .filter((s) => (seen.has(s) ? false : (seen.add(s), true)));
}
