import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
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

const STATUS_LABEL: Record<Status, string> = {
  fetching: "Analyse…",
  ready: "Prêt",
  downloading: "En cours",
  paused: "En pause",
  interrupted: "Interrompu",
  done: "Terminé",
  error: "Erreur",
};

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

function App() {
  const [tab, setTab] = useState<Tab>("youtube");
  const [url, setUrl] = useState("");
  const [kind, setKind] = useState<"video" | "audio">("video");
  const [fileUrl, setFileUrl] = useState("");
  const [outDir, setOutDir] = useState("");
  const [items, setItems] = useState<Record<string, DownloadInfo>>({});
  const [choice, setChoice] = useState<Record<string, string>>({});
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [dirWarn, setDirWarn] = useState(false);

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
      // Surface the failure on the card instead of leaving it stuck on "Analyse…".
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

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="logo">
            <IconLogo />
          </span>
          <div>
            <h1>TurboGrab</h1>
            <p>Téléchargeur — YouTube &amp; fichiers</p>
          </div>
        </div>
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
            <IconFile /> Fichier
          </button>
        </nav>
      </header>

      <main className="content">
        {interrupted.length > 0 && !bannerDismissed && (
          <div className="banner">
            <IconResume />
            <div className="banner-text">
              <strong>
                {interrupted.length} téléchargement
                {interrupted.length > 1 ? "s" : ""} interrompu
                {interrupted.length > 1 ? "s" : ""}
              </strong>
              <span>Reprendre là où ça s'est arrêté ?</span>
            </div>
            <button className="btn ghost" onClick={() => setBannerDismissed(true)}>
              Plus tard
            </button>
            <button className="btn accent" onClick={() => invoke("resume_all")}>
              Tout reprendre
            </button>
          </div>
        )}

        <section className="composer">
          {tab === "youtube" ? (
            <div className="composer-row">
              <input
                className="url"
                placeholder="Colle une URL YouTube…"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && fetchInfo()}
              />
              <div className="segment">
                <button
                  className={kind === "video" ? "on" : ""}
                  onClick={() => setKind("video")}
                >
                  <IconVideo /> Vidéo
                </button>
                <button
                  className={kind === "audio" ? "on" : ""}
                  onClick={() => setKind("audio")}
                >
                  <IconAudio /> Audio
                </button>
              </div>
              <button
                className="btn accent lg"
                onClick={fetchInfo}
                disabled={!url.trim()}
              >
                <IconSearch /> Récupérer
              </button>
            </div>
          ) : (
            <div className="composer-row">
              <input
                className="url"
                placeholder="Colle un lien de fichier direct…"
                value={fileUrl}
                onChange={(e) => setFileUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && startFile()}
              />
              <button
                className="btn accent lg"
                onClick={startFile}
                disabled={!fileUrl.trim()}
              >
                <IconLink /> Télécharger
              </button>
            </div>
          )}
          <button
            className={`dir${dirWarn ? " warn" : ""}`}
            onClick={pickDir}
            title={outDir}
          >
            <IconFolder />
            <span>{outDir || "Choisir un dossier…"}</span>
            <em>Changer</em>
          </button>
          {dirWarn && (
            <div className="errline dirwarn">
              <IconWarn />
              <span>Choisis d'abord un dossier de destination.</span>
            </div>
          )}
        </section>

        <div className="listhead">
          <span>
            {visible.length} élément{visible.length > 1 ? "s" : ""}
          </span>
          {finishedCount > 0 && (
            <button className="link" onClick={() => invoke("clear_finished")}>
              <IconClear /> Effacer les terminés
            </button>
          )}
        </div>

        <ul className="list">
          {visible.length === 0 && (
            <li className="empty">
              <IconDownload />
              <p>Aucun téléchargement pour l'instant.</p>
              <span>
                {tab === "file"
                  ? "Colle un lien de fichier direct ci-dessus."
                  : "Colle une URL YouTube ci-dessus pour commencer."}
              </span>
            </li>
          )}
          {visible.map((d) => (
            <li key={d.id} className={`card ${d.status}`}>
              <Thumb d={d} />

              <div className="body">
                <div className="line1">
                  <span className="title">{d.title || d.url}</span>
                  <span className={`pill ${d.status}`}>{STATUS_LABEL[d.status]}</span>
                </div>

                {/* PHASE 1 — analyse en cours */}
                {d.status === "fetching" && (
                  <div className="track indet">
                    <div className="bar" />
                  </div>
                )}

                {/* PHASE 1 — échec */}
                {d.status === "error" && (
                  <div className="errline">
                    <IconWarn />
                    <span>{d.error_msg || "Une erreur est survenue"}</span>
                  </div>
                )}

                {/* PHASE 2 — prêt : choix qualité */}
                {d.status === "ready" && (
                  <div className="pickrow">
                    <label className="select">
                      <select
                        value={choice[d.id] ?? d.quality}
                        onChange={(e) =>
                          setChoice((c) => ({ ...c, [d.id]: e.target.value }))
                        }
                      >
                        {d.qualities.map((q) => (
                          <option key={q.value || "best"} value={q.value}>
                            {d.kind === "audio" ? "🎵 " : "🎬 "}
                            {q.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <button className="btn accent" onClick={() => download(d)}>
                      <IconDownload /> Télécharger
                    </button>
                  </div>
                )}

                {/* PHASE 2 — barre de progression */}
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
                    title="Réessayer"
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
                    title="Pause"
                    onClick={() => invoke("pause_download", { id: d.id })}
                  >
                    <IconPause />
                  </button>
                )}
                {(d.status === "paused" || d.status === "interrupted") && (
                  <button
                    className="icon accent"
                    title="Reprendre"
                    onClick={() => invoke("resume_download", { id: d.id })}
                  >
                    <IconPlay />
                  </button>
                )}
                <button
                  className="icon danger"
                  title="Supprimer"
                  onClick={() => invoke("cancel_download", { id: d.id })}
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
