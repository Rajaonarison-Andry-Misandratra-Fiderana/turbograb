/** Mirrors `DownloadInfo` in src-tauri/src/lib.rs. Keep the two in step. */

export type Status =
  | "fetching"
  | "ready"
  | "downloading"
  | "paused"
  | "interrupted"
  | "done"
  | "error";

export type Kind = "video" | "audio" | "file";

export type Tab = "youtube" | "file";

export interface Quality {
  label: string;
  value: string;
  /** Estimated bytes; 0 = unknown. Formatted in the UI, per locale. */
  size_bytes: number;
}

export interface DownloadInfo {
  id: string;
  url: string;
  kind: Kind;
  out_dir: string;
  title: string;
  thumbnail: string;
  qualities: Quality[];
  quality: string;
  percent: number;
  speed: string;
  eta: string;
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
  duration: number;
  uploader: string;
}

/** Backend flags a 401/403/410 on a direct link as `@link_expired`: the bytes
 *  on disk are still good, only the address went stale. */
export const isExpired = (d: DownloadInfo) =>
  d.status === "error" && d.error_msg === "@link_expired";

export const isFinished = (d: DownloadInfo) =>
  d.status === "done" || d.status === "error";

export const isRunning = (d: DownloadInfo) =>
  d.status === "downloading" || d.status === "fetching";

/** Newest first, and stable: sorting by status made cards jump mid-download. */
export const byNewest = (a: DownloadInfo, b: DownloadInfo) =>
  b.created_at - a.created_at || a.id.localeCompare(b.id);
