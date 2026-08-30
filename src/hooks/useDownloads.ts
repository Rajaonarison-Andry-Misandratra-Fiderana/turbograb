import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { byNewest, type DownloadInfo } from "../types";

/** Owns the download list: the initial snapshot, the two backend event
 *  streams, and the imperative actions. Everything else reads from here. */
export function useDownloads(onDone?: (d: DownloadInfo) => void) {
  const [items, setItems] = useState<Record<string, DownloadInfo>>({});
  // Read inside the event handler without making it a dependency, so the
  // listener is registered exactly once.
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  useEffect(() => {
    invoke<DownloadInfo[]>("list_downloads").then((list) =>
      setItems(Object.fromEntries(list.map((d) => [d.id, d]))),
    );

    const unUpd = listen<DownloadInfo>("download-update", (e) =>
      setItems((prev) => {
        const before = prev[e.payload.id];
        if (before && before.status !== "done" && e.payload.status === "done") {
          doneRef.current?.(e.payload);
        }
        return { ...prev, [e.payload.id]: e.payload };
      }),
    );
    const unDel = listen<string>("download-removed", (e) =>
      setItems((prev) => {
        const next = { ...prev };
        delete next[e.payload];
        return next;
      }),
    );
    // React 19 StrictMode mounts effects twice; without these the second mount
    // would leave a duplicate listener behind and every card would double-update.
    return () => {
      unUpd.then((f) => f());
      unDel.then((f) => f());
    };
  }, []);

  const list = useMemo(() => Object.values(items).sort(byNewest), [items]);

  /** Put a card into an error state the backend never got to report — a
   *  command that rejected before the item existed on its side. */
  const failLocally = useCallback((seed: Partial<DownloadInfo> & { id: string }, e: unknown) => {
    setItems((prev) => ({
      ...prev,
      [seed.id]: {
        ...(prev[seed.id] ?? (seed as DownloadInfo)),
        ...seed,
        status: "error",
        error_msg: String(e),
      } as DownloadInfo,
    }));
  }, []);

  return { items, list, setItems, failLocally };
}
