import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import Box from "@mui/material/Box";
import Chip from "@mui/material/Chip";
import IconButton from "@mui/material/IconButton";
import InputAdornment from "@mui/material/InputAdornment";
import Snackbar from "@mui/material/Snackbar";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import TextField from "@mui/material/TextField";
import Tooltip from "@mui/material/Tooltip";
import { useColorScheme } from "@mui/material/styles";
import ClearOutlined from "@mui/icons-material/ClearOutlined";
import DeleteSweepOutlined from "@mui/icons-material/DeleteSweepOutlined";
import DownloadOutlined from "@mui/icons-material/DownloadOutlined";
import DownloadingOutlined from "@mui/icons-material/DownloadingOutlined";
import SearchOutlined from "@mui/icons-material/SearchOutlined";
import SettingsOutlined from "@mui/icons-material/SettingsOutlined";

import { Composer } from "./components/Composer";
import { ConfirmDialog, type ConfirmSpec } from "./components/ConfirmDialog";
import { DownloadCard, type CardActions } from "./components/DownloadCard";
import { EmptyState } from "./components/EmptyState";
import { LogDialog } from "./components/LogDialog";
import { PairDialog, type PairRequest } from "./components/PairDialog";
import { ResizeHandles } from "./components/ResizeHandles";
import { ResumeBanner } from "./components/ResumeBanner";
import { SettingsDialog } from "./components/SettingsDialog";
import { TitleBar } from "./components/TitleBar";
import { errorText } from "./errors";
import { useDownloads } from "./hooks/useDownloads";
import { DICT, type Lang } from "./i18n";
import { isFinished, type DownloadInfo, type Settings } from "./types";

/** Page gutter, in theme spacing units (8px each). */
const GUTTER = 2.5;

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const lang: Lang = settings?.lang === "en" ? "en" : "fr";
  const t = DICT[lang];
  const { mode } = useColorScheme();

  const [query, setQuery] = useState("");
  const [dirWarn, setDirWarn] = useState(false);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [logsOpen, setLogsOpen] = useState(false);
  const [confirm, setConfirm] = useState<ConfirmSpec | null>(null);
  const [pair, setPair] = useState<PairRequest | null>(null);
  const [toast, setToast] = useState("");
  const [present, setPresent] = useState<Record<string, boolean>>({});

  const { list, failLocally } = useDownloads(
    useCallback((d: DownloadInfo) => setToast(DICT[lang].toastDone(d.title || d.url)), [lang]),
  );

  // ---- settings: one round-trip on mount, then write-through ----
  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings).catch(() => {});
  }, []);

  // The window starts hidden (tauri.conf.json) and is revealed once React has
  // painted. A borderless window has no native background to fall back on, so
  // showing it earlier means a flash of the wrong colour in one scheme or the
  // other; waiting for the first frame is right for both.
  //
  // Except when this launch was meant to stay in the tray — at boot, or by
  // choice. The webview still runs, so downloads and the browser listener are
  // live; there is simply no window until the tray is clicked.
  useEffect(() => {
    let cancelled = false;
    invoke<boolean>("launched_hidden")
      .catch(() => false)
      .then((hidden) => {
        if (cancelled || hidden) return;
        requestAnimationFrame(() => {
          getCurrentWindow().show().catch(() => {});
        });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // A pairing request parks an HTTP connection until this dialog is answered,
  // so it is rendered wherever the app is — no route, no tab to be on.
  useEffect(() => {
    const unReq = listen<PairRequest>("pair-request", (e) => setPair(e.payload));
    const unClosed = listen<string>("pair-closed", (e) =>
      setPair((cur) => (cur && cur.id === e.payload ? null : cur)),
    );
    const unPaired = listen<string>("paired", (e) => {
      setSettings((cur) => (cur ? { ...cur, token: e.payload } : cur));
      setToast(e.payload ? DICT[langRef.current].pairedToast : "");
    });
    return () => {
      unReq.then((f) => f());
      unClosed.then((f) => f());
      unPaired.then((f) => f());
    };
  }, []);

  const patchSettings = useCallback((next: Partial<Settings>) => {
    setSettings((cur) => {
      if (!cur) return cur;
      const merged = { ...cur, ...next };
      invoke("set_settings", { settings: merged }).catch(() => {});
      return merged;
    });
  }, []);

  // Read by listeners that are registered exactly once, so they can phrase a
  // toast in the current language without re-subscribing on every change.
  const langRef = useRef<Lang>(lang);
  langRef.current = lang;

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  // The colour scheme is owned by MUI (it has to be readable before React
  // mounts). Mirror it into settings.json so one file holds every preference.
  const lastMode = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!settings || !mode || mode === lastMode.current) return;
    lastMode.current = mode;
    if (settings.theme !== mode) patchSettings({ theme: mode });
  }, [mode, settings, patchSettings]);

  const outDir = settings?.out_dir ?? "";

  // ---- derived views ----
  // One list: the tab split is gone, so a counter can no longer disagree with
  // what is on screen. Only the search box narrows what is shown.
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (d) => d.title.toLowerCase().includes(q) || d.url.toLowerCase().includes(q),
    );
  }, [list, query]);

  const downloading = useMemo(
    () => list.filter((d) => d.status === "downloading"),
    [list],
  );
  const interrupted = useMemo(
    () => list.filter((d) => d.status === "interrupted"),
    [list],
  );
  const finished = useMemo(() => list.filter(isFinished), [list]);

  // ---- which finished files are still on disk ----
  const doneIds = useMemo(
    () => list.filter((d) => d.status === "done").map((d) => d.id),
    [list],
  );
  const doneKey = doneIds.join(",");
  useEffect(() => {
    if (!doneIds.length) return setPresent({});
    invoke<boolean[]>("files_present", { ids: doneIds })
      .then((flags) =>
        setPresent(Object.fromEntries(doneIds.map((id, i) => [id, flags[i]]))),
      )
      .catch(() => {});
    // doneKey is the stable identity of doneIds; depending on the array itself
    // would refire on every list update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doneKey]);

  // ---- actions ----
  const pickDir = async () => {
    const dir = await openDialog({ directory: true, defaultPath: outDir || undefined });
    if (typeof dir === "string") {
      patchSettings({ out_dir: dir });
      setDirWarn(false);
    }
  };

  // Commands reject with the same "@code" vocabulary the cards use, so a failed
  // action reads the same wherever it surfaces.
  const fail = (e: unknown) => setToast(errorText(String(e), t));

  const submit = async (urls: string[]) => {
    if (!outDir) return setDirWarn(true);
    try {
      await invoke("add_downloads", { urls, outDir });
    } catch (e) {
      // Surface it on a card instead of leaving the user with nothing.
      failLocally({ id: crypto.randomUUID(), url: urls[0] }, e);
    }
  };

  const actions: CardActions = {
    pause: (d) => {
      const run = () => invoke("pause_download", { id: d.id }).catch(fail);
      // A server that ignores Range makes "pause" mean "throw away the bytes".
      if (d.resumable === false) {
        setConfirm({
          title: t.pause,
          body: t.notResumable,
          confirmLabel: t.pause,
          cancelLabel: t.cancelAction,
          destructive: true,
          onConfirm: run,
        });
      } else run();
    },
    resume: (d) => invoke("resume_download", { id: d.id }).catch(fail),
    retry: (d) => invoke("resume_download", { id: d.id }).catch(fail),
    relink: (d, url) => invoke("refresh_link", { id: d.id, url }).catch(fail),
    openFile: (d) => invoke("open_file", { id: d.id }).catch(fail),
    revealFile: (d) => invoke("reveal_file", { id: d.id }).catch(fail),
    copy: (text) => {
      navigator.clipboard.writeText(text).then(
        () => setToast(t.copied),
        () => {},
      );
    },
    remove: (d) => {
      const running = !isFinished(d);
      setConfirm({
        title: running ? t.confirmStopTitle : t.confirmRemoveTitle,
        body: running ? t.confirmStopBody : t.confirmRemoveBody,
        confirmLabel: running ? t.confirmStop : t.confirmRemove,
        cancelLabel: t.cancelAction,
        // Offer the file deletion only when there is a file to delete.
        checkboxLabel:
          d.status === "done" || d.percent > 0 ? t.alsoDeleteFile : undefined,
        destructive: running,
        onConfirm: (deleteFile) =>
          invoke("cancel_download", { id: d.id, deleteFile }).catch(fail),
      });
    },
  };

  const clearFinished = () => {
    setConfirm({
      title: t.confirmClearTitle,
      body: t.confirmClearBody(finished.length),
      confirmLabel: t.confirmClear,
      cancelLabel: t.cancelAction,
      checkboxLabel: t.alsoDeleteFiles,
      onConfirm: (deleteFiles) => {
        invoke("clear_finished", { deleteFiles }).catch(fail);
      },
    });
  };

  return (
    <Box sx={{ height: "100%", display: "flex", flexDirection: "column" }}>
      <ResizeHandles />

      <TitleBar
        closeLabel={t.close}
        actions={
          downloading.length > 0 && (
            <Tooltip title={t.activeSummary(downloading.length)}>
              <Chip
                size="small"
                color="primary"
                icon={<DownloadingOutlined />}
                label={downloading.length}
                sx={{ fontVariantNumeric: "tabular-nums" }}
              />
            </Tooltip>
          )
        }
      />

      {/* Outside the scroll area on purpose: pasting a link must never require
          scrolling back up past a long list. */}
      {/* One gutter value on all four sides of the content column: the page
          padding, the gap between composer and list, and the space under the
          last card are all the same 20px. */}
      <Box sx={{ px: GUTTER, pt: GUTTER, flexShrink: 0 }}>
        <Box sx={{ maxWidth: 720, mx: "auto" }}>
          <Composer
            t={t}
            outDir={outDir}
            dirWarn={dirWarn}
            onPickDir={pickDir}
            onSubmit={submit}
          />
        </Box>
      </Box>

      {/* The flex chain runs all the way down to EmptyState, which is what lets
          it fill the leftover height and centre inside it. */}
      <Box
        sx={{
          flex: 1,
          minHeight: 0,
          overflowY: "auto",
          px: GUTTER,
          pt: GUTTER,
          pb: GUTTER,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <Box
          sx={{
            maxWidth: 720,
            width: "100%",
            mx: "auto",
            flex: 1,
            display: "flex",
            flexDirection: "column",
          }}
        >
          <ResumeBanner
            count={bannerDismissed ? 0 : interrupted.length}
            t={t}
            onDismiss={() => setBannerDismissed(true)}
            onResumeAll={() => invoke("resume_all").catch(fail)}
          />

          <Stack
            direction="row"
            spacing={1}
            sx={{ mb: 2, alignItems: "center", flexShrink: 0 }}
          >
            <Typography
              variant="caption"
              color="text.secondary"
              noWrap
              sx={{ flex: 1, minWidth: 0 }}
            >
              {list.length > 0 && t.summary(list.length, downloading.length)}
            </Typography>
            {list.length > 1 && (
              <TextField
                placeholder={t.search}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                sx={{ width: 200 }}
                slotProps={{
                  input: {
                    startAdornment: (
                      <InputAdornment position="start">
                        <SearchOutlined fontSize="small" />
                      </InputAdornment>
                    ),
                    endAdornment: query ? (
                      <InputAdornment position="end">
                        <IconButton
                          size="small"
                          aria-label={t.clearSearch}
                          onClick={() => setQuery("")}
                        >
                          <ClearOutlined fontSize="small" />
                        </IconButton>
                      </InputAdornment>
                    ) : null,
                  },
                }}
              />
            )}
            {finished.length > 0 && (
              <Tooltip title={t.clearFinished}>
                <IconButton aria-label={t.clearFinished} onClick={clearFinished}>
                  <DeleteSweepOutlined />
                </IconButton>
              </Tooltip>
            )}
            <Tooltip title={t.settings}>
              <IconButton aria-label={t.settings} onClick={() => setSettingsOpen(true)}>
                <SettingsOutlined />
              </IconButton>
            </Tooltip>
          </Stack>

          <Stack
            component="ul"
            spacing={1.5}
            sx={{ m: 0, p: 0, flex: 1, display: "flex", flexDirection: "column" }}
          >
            {visible.length === 0 &&
              (list.length === 0 ? (
                <EmptyState
                  icon={<DownloadOutlined />}
                  title={t.emptyTitle}
                  hint={t.emptyHint}
                />
              ) : (
                <EmptyState icon={<SearchOutlined />} title={t.noMatch} hint={query} />
              ))}
            {visible.map((d) => (
              <DownloadCard
                key={d.id}
                d={d}
                t={t}
                lang={lang}
                filePresent={present[d.id]}
                actions={actions}
              />
            ))}
          </Stack>
        </Box>
      </Box>

      {settings && (
        <SettingsDialog
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          t={t}
          lang={lang}
          settings={settings}
          patch={patchSettings}
          onPickDir={pickDir}
          onOpenLogs={() => setLogsOpen(true)}
          onCopy={actions.copy}
          onMakeToken={() =>
            invoke<string>("ensure_token")
              .then((token) => setSettings((cur) => (cur ? { ...cur, token } : cur)))
              .catch(fail)
          }
          onRevokeToken={() =>
            setConfirm({
              title: t.revoke,
              body: t.revokeBody,
              confirmLabel: t.revoke,
              cancelLabel: t.cancelAction,
              destructive: true,
              onConfirm: () => {
                invoke("revoke_token").catch(fail);
                setSettings((cur) => (cur ? { ...cur, token: "" } : cur));
              },
            })
          }
        />
      )}
      <LogDialog
        open={logsOpen}
        onClose={() => setLogsOpen(false)}
        t={t}
        lang={lang}
        onCopy={actions.copy}
      />
      <ConfirmDialog spec={confirm} onClose={() => setConfirm(null)} />
      <PairDialog
        req={pair}
        t={t}
        onRespond={(id, allow) => {
          setPair(null);
          invoke("pair_respond", { id, allow }).catch(fail);
        }}
      />
      <Snackbar
        open={!!toast}
        message={toast}
        autoHideDuration={4000}
        onClose={() => setToast("")}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      />
    </Box>
  );
}
