// FR/EN dictionary. `t = DICT[lang]`; count-dependent strings are functions.
// Units, dates and numbers are NOT here — they live in format.ts, which
// derives them from the locale.
export type Lang = "fr" | "en";

export const LANGS: { id: Lang; label: string }[] = [
  { id: "fr", label: "Français" },
  { id: "en", label: "English" },
];

const fr = {
  // ---- chrome ----
  settings: "Paramètres",
  language: "Langue",
  theme: "Thème",
  themeSystem: "Système",
  themeLight: "Clair",
  themeDark: "Sombre",
  close: "Fermer",
  logs: "Journal",
  viewLogs: "Voir le journal",
  logHint:
    "Le détail brut des échecs. Les cartes n'affichent qu'un message court : c'est ici que se trouve le texte exact, à joindre à un rapport de bug.",
  logEmpty: "Rien à signaler pour cette session.",
  copyAll: "Tout copier",
  clearLog: "Vider",

  // ---- composer ----
  urlLabel: "Lien",
  urlPlaceholder: "Colle un lien YouTube ou un fichier…",
  sourceKind: "Type de téléchargement",
  directFile: "Fichier direct",
  detected: "détecté",
  paste: "Coller",
  clearField: "Effacer",
  video: "Vidéo",
  audio: "Audio",
  download: "Télécharger",
  destination: "Destination",
  chooseDir: "Choisir un dossier…",
  change: "Changer",
  dirWarn: "Choisis d'abord un dossier de destination.",

  // ---- banner ----
  interruptedTitle: (n: number) =>
    `${n} téléchargement${n > 1 ? "s" : ""} interrompu${n > 1 ? "s" : ""}`,
  resumePrompt: "Reprendre là où ça s'est arrêté ?",
  later: "Plus tard",
  resumeAll: "Tout reprendre",

  // ---- list ----
  clearFinished: "Effacer les terminés",
  search: "Filtrer…",
  emptyTitle: "Aucun téléchargement pour l'instant.",
  emptyHint: "Colle un lien ci-dessus : YouTube ou fichier direct, on reconnaît tout seul.",
  summary: (total: number, active: number) =>
    active > 0
      ? `${total} téléchargement${total > 1 ? "s" : ""} · ${active} en cours`
      : `${total} téléchargement${total > 1 ? "s" : ""}`,
  noMatch: "Aucun résultat pour ce filtre.",
  clearSearch: "Effacer le filtre",

  // ---- card ----
  best: "Meilleure",
  quality: "Qualité",
  unknownSize: "Taille inconnue",
  retry: "Réessayer",
  pause: "Pause",
  resume: "Reprendre",
  delete: "Supprimer",
  more: "Plus d'actions",
  openFile: "Ouvrir le fichier",
  revealFile: "Afficher dans le dossier",
  copyLink: "Copier le lien source",
  copyPath: "Copier le chemin",
  copyError: "Copier le détail de l'erreur",
  notResumable: "Ce serveur ne gère pas la reprise : une pause repartira de zéro.",
  retrying: (n: number, max: number) => `Nouvelle tentative ${n}/${max}…`,
  connections: (n: number) => `${n} connexion${n > 1 ? "s" : ""}`,
  singleStream: "1 connexion",
  waiting: "En attente…",
  fileGone: "Fichier introuvable sur le disque",

  // ---- summary ----
  activeSummary: (n: number) =>
    `${n} téléchargement${n > 1 ? "s" : ""} en cours`,
  nothingActive: "Aucun téléchargement actif",

  // ---- expired link ----
  relinkHint:
    "Ce lien a expiré. Colle un lien à jour : ce qui est déjà téléchargé est conservé.",
  relinkPlaceholder: "Colle le nouveau lien…",
  relinkBtn: "Reprendre",

  // ---- dialogs ----
  cancelAction: "Annuler",
  confirmStopTitle: "Annuler ce téléchargement ?",
  confirmStopBody:
    "Le transfert en cours sera arrêté et l'élément retiré de la liste.",
  confirmStop: "Arrêter",
  confirmRemoveTitle: "Retirer de la liste ?",
  confirmRemoveBody: "Le fichier déjà téléchargé reste sur le disque.",
  confirmRemove: "Retirer",
  alsoDeleteFile: "Supprimer aussi le fichier du disque",
  alsoDeleteFiles: "Supprimer aussi les fichiers du disque",
  confirmClearTitle: "Effacer les téléchargements terminés ?",
  confirmClearBody: (n: number) =>
    `${n} élément${n > 1 ? "s" : ""} (terminés et en erreur) ${
      n > 1 ? "seront retirés" : "sera retiré"
    } de la liste.`,
  confirmClear: "Effacer",

  // ---- toasts ----
  copied: "Copié",
  toastDone: (title: string) => `Terminé : ${title}`,

  status: {
    fetching: "Analyse…",
    ready: "Prêt",
    downloading: "En cours",
    paused: "En pause",
    interrupted: "Interrompu",
    done: "Terminé",
    error: "Erreur",
  } as Record<string, string>,

  // Fixed backend error codes (sent as "@code"); dynamic yt-dlp text passes through.
  genericError: "Une erreur est survenue",
  errors: {
    fetch_interrupted: "Analyse interrompue",
    analyze_failed: "Impossible d'analyser cette URL",
    unreadable: "Réponse illisible de yt-dlp",
    download_failed: "Le téléchargement a échoué",
    no_ffmpeg: "ffmpeg introuvable — réinstalle l'application",
    link_expired: "Lien expiré",
    no_range: "Le serveur a refusé le téléchargement par segments",
    bad_link: "Lien invalide",
    busy: "Mets d'abord le téléchargement en pause",
    short_read: "Le serveur a coupé l'envoi avant la fin",
    network: "Erreur réseau — vérifie ta connexion",
    server: "Le serveur a répondu par une erreur",
    not_found: "Contenu introuvable ou supprimé",
    blocked: "Accès refusé par le service (connexion, âge ou région)",
    rate_limited: "Trop de requêtes — réessaie dans quelques minutes",
    unsupported: "Lien non pris en charge",
    no_space: "Plus d'espace disque",
    no_write: "Écriture impossible dans ce dossier",
    missing_file: "Fichier introuvable sur le disque",
  } as Record<string, string>,
};

export type Dict = typeof fr;

const en: Dict = {
  settings: "Settings",
  language: "Language",
  theme: "Theme",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",
  close: "Close",
  logs: "Log",
  viewLogs: "View log",
  logHint:
    "The raw detail behind failures. Cards only show a short message; the exact text lives here, ready to attach to a bug report.",
  logEmpty: "Nothing to report this session.",
  copyAll: "Copy all",
  clearLog: "Clear",

  urlLabel: "Link",
  urlPlaceholder: "Paste a YouTube or file link…",
  sourceKind: "Download type",
  directFile: "Direct file",
  detected: "detected",
  paste: "Paste",
  clearField: "Clear",
  video: "Video",
  audio: "Audio",
  download: "Download",
  destination: "Destination",
  chooseDir: "Choose a folder…",
  change: "Change",
  dirWarn: "Choose a destination folder first.",

  interruptedTitle: (n) => `${n} interrupted download${n > 1 ? "s" : ""}`,
  resumePrompt: "Resume where it left off?",
  later: "Later",
  resumeAll: "Resume all",

  clearFinished: "Clear finished",
  search: "Filter…",
  emptyTitle: "No downloads yet.",
  emptyHint: "Paste a link above — YouTube or a direct file, we work out which.",
  summary: (total, active) =>
    active > 0
      ? `${total} download${total > 1 ? "s" : ""} · ${active} in progress`
      : `${total} download${total > 1 ? "s" : ""}`,
  noMatch: "Nothing matches this filter.",
  clearSearch: "Clear filter",

  best: "Best",
  quality: "Quality",
  unknownSize: "Unknown size",
  retry: "Retry",
  pause: "Pause",
  resume: "Resume",
  delete: "Delete",
  more: "More actions",
  openFile: "Open file",
  revealFile: "Show in folder",
  copyLink: "Copy source link",
  copyPath: "Copy path",
  copyError: "Copy error details",
  notResumable: "This server can't resume: pausing will restart from zero.",
  retrying: (n, max) => `Retrying ${n}/${max}…`,
  connections: (n) => `${n} connection${n > 1 ? "s" : ""}`,
  singleStream: "1 connection",
  waiting: "Waiting…",
  fileGone: "File not found on disk",

  activeSummary: (n) => `${n} download${n > 1 ? "s" : ""} in progress`,
  nothingActive: "No active downloads",

  relinkHint:
    "This link has expired. Paste an up-to-date one — what's already downloaded is kept.",
  relinkPlaceholder: "Paste the new link…",
  relinkBtn: "Resume",

  cancelAction: "Cancel",
  confirmStopTitle: "Cancel this download?",
  confirmStopBody:
    "The transfer in progress will stop and the item will leave the list.",
  confirmStop: "Stop",
  confirmRemoveTitle: "Remove from the list?",
  confirmRemoveBody: "The downloaded file stays on disk.",
  confirmRemove: "Remove",
  alsoDeleteFile: "Also delete the file from disk",
  alsoDeleteFiles: "Also delete the files from disk",
  confirmClearTitle: "Clear finished downloads?",
  confirmClearBody: (n) =>
    `${n} item${n > 1 ? "s" : ""} (done and failed) will leave the list.`,
  confirmClear: "Clear",

  copied: "Copied",
  toastDone: (title) => `Done: ${title}`,

  status: {
    fetching: "Analyzing…",
    ready: "Ready",
    downloading: "Downloading",
    paused: "Paused",
    interrupted: "Interrupted",
    done: "Done",
    error: "Error",
  },

  genericError: "Something went wrong",
  errors: {
    fetch_interrupted: "Analysis interrupted",
    analyze_failed: "Couldn't analyze this URL",
    unreadable: "Unreadable yt-dlp response",
    download_failed: "Download failed",
    no_ffmpeg: "ffmpeg not found — reinstall the app",
    link_expired: "Link expired",
    no_range: "Server refused the segmented download",
    bad_link: "Invalid link",
    busy: "Pause the download first",
    short_read: "The server cut the transfer short",
    network: "Network error — check your connection",
    server: "The server answered with an error",
    not_found: "Content not found or removed",
    blocked: "The service refused access (sign-in, age or region)",
    rate_limited: "Too many requests — try again in a few minutes",
    unsupported: "Unsupported link",
    no_space: "No disk space left",
    no_write: "Can't write to this folder",
    missing_file: "File not found on disk",
  },
};

export const DICT: Record<Lang, Dict> = { fr, en };
