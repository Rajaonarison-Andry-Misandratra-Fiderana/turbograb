import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, ask } from "@tauri-apps/plugin-dialog";
import { downloadDir } from "@tauri-apps/api/path";
import {
  IconAudio,
  IconClear,
  IconDownload,
  IconFile,
  IconFolder,
  IconLink,
  IconLogo,
  IconPause,
  IconPlay,
  IconResume,
  IconSearch,
  IconVideo,
  IconWarn,
  IconX,
} from "./icons";
import { DICT, type Lang } from "./i18n";
import "./App.css";

type Status =
  | "fetching"
  | "ready"
  | "downloading"
  | "paused"
  | "interrupted"
  | "done"
  | "error";

interface Quality {
  label: string;
  value: string;
  size: string;
}

type Tab = "youtube" | "file";

interface DownloadInfo {
  id: string;
  url: string;
  kind: "video" | "audio" | "file";
  out_dir: string;
  title: string;
  thumbnail: string;
  qualities: Quality[];
  quality: string;
  percent: number;
  speed: string;
  eta: string;
  error_msg: string;
  status: Status;
}

const ORDER: Record<Status, number> = {
  fetching: 0,
  ready: 1,
  downloading: 2,
  interrupted: 3,
  paused: 4,
  error: 5,
  done: 6,
};

function Thumb({ d }: { d: DownloadInfo }) {
  if (d.thumbnail) {
    return (
      <span className="thumb img">
        <img src={d.thumbnail} alt="" loading="lazy" />
      </span>
    );
  }
  return (
    <span className={`thumb ${d.kind}`}>
      {d.kind === "audio" ? (
        <IconAudio />
      ) : d.kind === "file" ? (
        <IconFile />
      ) : (
        <IconVideo />
      )}
    </span>
  );
}

// Custom quality picker: label on the left, estimated size right-aligned.
// A native <select> can't right-align per-option text, hence the popover.
function QualitySelect({
  d,
  value,
  onChange,
  best,
}: {
  d: DownloadInfo;
  value: string;
  onChange: (v: string) => void;
  best: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const prefix = d.kind === "audio" ? "🎵 " : "🎬 ";
  const labelOf = (q: Quality) => (q.value === "" ? best : q.label);
  const cur = d.qualities.find((q) => q.value === value) ?? d.qualities[0];

  return (
    <div className={`qsel${open ? " open" : ""}`} ref={ref}>
      <button
        type="button"
        className="qsel-btn"
        onClick={() => setOpen((o) => !o)}
      >
        <span className="qsel-label">
          {prefix}
          {cur ? labelOf(cur) : ""}
        </span>
        {cur?.size && <span className="qsel-size">{cur.size}</span>}
        <span className="qsel-caret">▾</span>
      </button>
      {open && (
        <ul className="qsel-list">
          {d.qualities.map((q) => (
            <li key={q.value || "best"}>
              <button
                type="button"
                className={q.value === value ? "on" : ""}
                onClick={() => {
                  onChange(q.value);
                  setOpen(false);
                }}
              >
                <span className="qsel-label">
                  {prefix}
                  {labelOf(q)}
                </span>
                {q.size && <span className="qsel-size">{q.size}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function App() {
  const [lang, setLang] = useState<Lang>(
    () => (localStorage.getItem("lang") as Lang) || "fr",
  );
  const t = DICT[lang];
  // Backend sends fixed messages as "@code"; translate those, show the rest raw.
  const errText = (msg: string) =>
    msg.startsWith("@")
      ? t.errors[msg.slice(1)] ?? t.genericError
      : msg || t.genericError;

  const [tab, setTab] = useState<Tab>("youtube");
  const [url, setUrl] = useState("");
  const [kind, setKind] = useState<"video" | "audio">("video");
  const [fileUrl, setFileUrl] = useState("");
  const [outDir, setOutDir] = useState("");
  const [items, setItems] = useState<Record<string, DownloadInfo>>({});
  const [choice, setChoice] = useState<Record<string, string>>({});
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [dirWarn, setDirWarn] = useState(false);

  function toggleLang() {
    setLang((l) => {
      const next = l === "fr" ? "en" : "fr";
      localStorage.setItem("lang", next);
      return next;
    });
  }

  useEffect(() => {
    downloadDir().then(setOutDir).catch(() => {});
    invoke<DownloadInfo[]>("list_downloads").then((list) =>
      setItems(Object.fromEntries(list.map((d) => [d.id, d]))),
    );

    const unUpd = listen<DownloadInfo>("download-update", (e) =>
      setItems((prev) => ({ ...prev, [e.payload.id]: e.payload })),
    );
    const unDel = listen<string>("download-removed", (e) =>
      setItems((prev) => {
        const next = { ...prev };
        delete next[e.payload];
        return next;
      }),
    );
    return () => {
      unUpd.then((f) => f());
      unDel.then((f) => f());
    };
  }, []);

  const list = useMemo(
    () => Object.values(items).sort((a, b) => ORDER[a.status] - ORDER[b.status]),
    [items],
  );
  // Each tab shows only its own downloads: YouTube (video/audio) vs direct files.
  const visible = useMemo(
    () => list.filter((d) => (tab === "file" ? d.kind === "file" : d.kind !== "file")),
    [list, tab],
  );
  const interrupted = list.filter((d) => d.status === "interrupted");
  const finishedCount = list.filter(
    (d) => d.status === "done" || d.status === "error",
  ).length;

  async function pickDir() {
    const dir = await open({ directory: true, defaultPath: outDir });
    if (typeof dir === "string") {
      setOutDir(dir);
      setDirWarn(false);
    }
  }

  async function fetchInfo() {
    if (!url.trim()) return;
    // No folder = the download would silently fail later. Warn up front.
    if (!outDir) {
      setDirWarn(true);
      return;
    }
    const id = crypto.randomUUID();
    try {
      await invoke("fetch_info", { id, url: url.trim(), kind });
      setUrl("");
    } catch (e) {
      // Surface the failure on the card instead of leaving it stuck on "fetching".
      setItems((prev) => ({
        ...prev,
        [id]: {
          ...(prev[id] ?? ({ id, url: url.trim(), kind } as DownloadInfo)),
          status: "error",
          error_msg: String(e),
        } as DownloadInfo,
      }));
    }
  }

  async function startFile() {
    if (!fileUrl.trim()) return;
    if (!outDir) {
      setDirWarn(true);
      return;
    }
    const id = crypto.randomUUID();
    try {
      await invoke("start_file_download", { id, url: fileUrl.trim(), outDir });
      setFileUrl("");
    } catch (e) {
      setItems((prev) => ({
        ...prev,
        [id]: {
          ...(prev[id] ?? ({ id, url: fileUrl.trim(), kind: "file" } as DownloadInfo)),
          status: "error",
          error_msg: String(e),
        } as DownloadInfo,
      }));
    }
  }

  async function download(d: DownloadInfo) {
    if (!outDir) {
      setDirWarn(true);
      return;
    }
    const quality = choice[d.id] ?? d.quality;
    await invoke("start_download", { id: d.id, quality, outDir });
  }

  // Deleting a finished item asks whether to remove the file from disk too.
  async function removeItem(d: DownloadInfo) {
    let deleteFile = false;
    if (d.status === "done") {
      deleteFile = await ask(t.askDeleteFile, { title: "TurboGrab", kind: "warning" });
    }
    await invoke("cancel_download", { id: d.id, deleteFile });
  }

  async function clearFinished() {
    const deleteFiles = await ask(t.askDeleteFilesBulk, {
      title: "TurboGrab",
      kind: "warning",
    });
    await invoke("clear_finished", { deleteFiles });
  }

  return (
    <div className="app">
      <header className="topbar" data-tauri-drag-region>
        <div className="brand">
          <span className="logo">
            <IconLogo />
          </span>
          <div>
            <h1>TurboGrab</h1>
            <p>{t.tagline}</p>
          </div>
        </div>
        <div className="topright">
          <button className="langtog" onClick={toggleLang} title="FR / EN">
            {lang.toUpperCase()}
          </button>
          <nav className="tabs">
            <button
              className={tab === "youtube" ? "on" : ""}
              onClick={() => setTab("youtube")}
            >
              <IconVideo /> YouTube
            </button>
            <button
              className={tab === "file" ? "on" : ""}
              onClick={() => setTab("file")}
            >
              <IconFile /> {t.tabFile}
            </button>
          </nav>
        </div>
      </header>

      <main className="content">
        {interrupted.length > 0 && !bannerDismissed && (
          <div className="banner">
            <IconResume />
            <div className="banner-text">
              <strong>{t.interruptedTitle(interrupted.length)}</strong>
              <span>{t.resumePrompt}</span>
            </div>
            <button className="btn ghost" onClick={() => setBannerDismissed(true)}>
              {t.later}
            </button>
            <button className="btn accent" onClick={() => invoke("resume_all")}>
              {t.resumeAll}
            </button>
          </div>
        )}

        <section className="composer">
          {tab === "youtube" ? (
            <div className="composer-row">
              <input
                className="url"
                placeholder={t.ytPlaceholder}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && fetchInfo()}
              />
              <div className="segment">
                <button
                  className={kind === "video" ? "on" : ""}
                  onClick={() => setKind("video")}
                >
                  <IconVideo /> {t.video}
                </button>
                <button
                  className={kind === "audio" ? "on" : ""}
                  onClick={() => setKind("audio")}
                >
                  <IconAudio /> {t.audio}
                </button>
              </div>
              <button
                className="btn accent lg"
                onClick={fetchInfo}
                disabled={!url.trim()}
              >
                <IconSearch /> {t.fetch}
              </button>
            </div>
          ) : (
            <div className="composer-row">
              <input
                className="url"
                placeholder={t.filePlaceholder}
                value={fileUrl}
                onChange={(e) => setFileUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && startFile()}
              />
              <button
                className="btn accent lg"
                onClick={startFile}
                disabled={!fileUrl.trim()}
              >
                <IconLink /> {t.download}
              </button>
            </div>
          )}
          <button
            className={`dir${dirWarn ? " warn" : ""}`}
            onClick={pickDir}
            title={outDir}
          >
            <IconFolder />
            <span>{outDir || t.chooseDir}</span>
            <em>{t.change}</em>
          </button>
          {dirWarn && (
            <div className="errline dirwarn">
              <IconWarn />
              <span>{t.dirWarn}</span>
            </div>
          )}
        </section>

        <div className="listhead">
          <span>{t.itemsLabel(visible.length)}</span>
          {finishedCount > 0 && (
            <button className="link" onClick={clearFinished}>
              <IconClear /> {t.clearFinished}
            </button>
          )}
        </div>

        <ul className="list">
          {visible.length === 0 && (
            <li className="empty">
              <IconDownload />
              <p>{t.emptyTitle}</p>
              <span>{tab === "file" ? t.emptyFile : t.emptyYt}</span>
            </li>
          )}
          {visible.map((d) => (
            <li key={d.id} className={`card ${d.status}`}>
              <Thumb d={d} />

              <div className="body">
                <div className="line1">
                  <span className="title">{d.title || d.url}</span>
                  <span className={`pill ${d.status}`}>{t.status[d.status]}</span>
                </div>

                {/* fetching */}
                {d.status === "fetching" && (
                  <div className="track indet">
                    <div className="bar" />
                  </div>
                )}

                {/* fetch error */}
                {d.status === "error" && (
                  <div className="errline">
                    <IconWarn />
                    <span>{errText(d.error_msg)}</span>
                  </div>
                )}

                {/* ready: quality pick */}
                {d.status === "ready" && (
                  <div className="pickrow">
                    <QualitySelect
                      d={d}
                      value={choice[d.id] ?? d.quality}
                      onChange={(v) => setChoice((c) => ({ ...c, [d.id]: v }))}
                      best={t.best}
                    />
                    <button className="btn accent" onClick={() => download(d)}>
                      <IconDownload /> {t.download}
                    </button>
                  </div>
                )}

                {/* progress */}
                {(d.status === "downloading" ||
                  d.status === "paused" ||
                  d.status === "interrupted" ||
                  d.status === "done") && (
                  <>
                    <div className="track">
                      <div
                        className="bar"
                        style={{ width: `${Math.min(d.percent, 100)}%` }}
                      />
                    </div>
                    <div className="line2">
                      <span>{d.percent.toFixed(d.percent < 100 ? 1 : 0)}%</span>
                      {d.status === "downloading" && (
                        <span>
                          {d.speed} · ETA {d.eta}
                        </span>
                      )}
                    </div>
                  </>
                )}
              </div>

              <div className="tools">
                {d.status === "error" && (
                  <button
                    className="icon accent"
                    title={t.retry}
                    onClick={() =>
                      invoke(d.kind === "file" ? "resume_download" : "retry_fetch", {
                        id: d.id,
                      })
                    }
                  >
                    <IconResume />
                  </button>
                )}
                {d.status === "downloading" && (
                  <button
                    className="icon"
                    title={t.pause}
                    onClick={() => invoke("pause_download", { id: d.id })}
                  >
                    <IconPause />
                  </button>
                )}
                {(d.status === "paused" || d.status === "interrupted") && (
                  <button
                    className="icon accent"
                    title={t.resume}
                    onClick={() => invoke("resume_download", { id: d.id })}
                  >
                    <IconPlay />
                  </button>
                )}
                <button
                  className="icon danger"
                  title={t.delete}
                  onClick={() => removeItem(d)}
                >
                  <IconX />
                </button>
              </div>
            </li>
          ))}
        </ul>
      </main>
    </div>
  );
}

export default App;
