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
  urlPlaceholder: "Colle un ou plusieurs liens…",
  paste: "Coller",
  clearField: "Effacer",
  download: "Télécharger",
  destination: "Destination",
  chooseDir: "Choisir un dossier…",
  change: "Changer",
  dirWarn: "Choisis d'abord un dossier de destination.",
  linkCount: (n: number) => `${n} liens détectés — Maj+Entrée pour aller à la ligne`,
  noLink: "Aucun lien http(s) reconnu.",

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
  emptyHint:
    "Colle un lien ci-dessus, ou installe l'extension navigateur pour que tes téléchargements arrivent ici tout seuls.",
  summary: (total: number, active: number) =>
    active > 0
      ? `${total} téléchargement${total > 1 ? "s" : ""} · ${active} en cours`
      : `${total} téléchargement${total > 1 ? "s" : ""}`,
  noMatch: "Aucun résultat pour ce filtre.",
  clearSearch: "Effacer le filtre",

  // ---- card ----
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
  queuedHint: "En file — démarre dès qu'une place se libère.",
  viaBrowser: "depuis le navigateur",
  fileGone: "Fichier introuvable sur le disque",

  // ---- summary ----
  activeSummary: (n: number) => `${n} téléchargement${n > 1 ? "s" : ""} en cours`,
  nothingActive: "Aucun téléchargement actif",

  // ---- expired link ----
  relinkHint:
    "Ce lien a expiré. Colle un lien à jour : ce qui est déjà téléchargé est conservé.",
  relinkPlaceholder: "Colle le nouveau lien…",
  relinkBtn: "Reprendre",

  // ---- settings: transfers ----
  transfers: "Transferts",
  connectionsSetting: "Connexions par téléchargement",
  connectionsHint:
    "Découpe le fichier et le récupère en parallèle. Mets 1 si un serveur bloque ou limite les requêtes multiples.",
  maxActiveSetting: "Téléchargements simultanés",
  maxActiveHint: "Les suivants attendent en file au lieu de se partager la ligne.",

  // ---- settings: startup ----
  startup: "Démarrage",
  autostart: "Lancer au démarrage",
  autostartHint: "TurboGrab s'ouvre avec la session, directement dans la zone de notification.",
  startHidden: "Démarrer en arrière-plan",
  startHiddenHint:
    "Aucune fenêtre au lancement : l'icône de la zone de notification l'ouvre quand tu en as besoin.",

  // ---- settings: browser ----
  browserSection: "Extension navigateur",
  serverEnabled: "Recevoir les téléchargements du navigateur",
  serverEnabledHint:
    "TurboGrab écoute sur 127.0.0.1 — la machine seulement, jamais le réseau.",
  serverPort: "Port",
  serverPortHint: "Doit correspondre au port réglé dans l'extension.",
  paired: "Extension appairée",
  pairedHint: "Le jeton ci-dessous autorise l'extension à envoyer des téléchargements.",
  notPaired: "Aucune extension appairée",
  notPairedHint:
    "Clique sur « Connecter » dans l'extension : une demande d'autorisation s'affichera ici.",
  copyToken: "Copier le jeton",
  makeToken: "Générer un jeton à coller dans l'extension",
  revoke: "Révoquer",
  revokeBody:
    "L'extension ne pourra plus envoyer de téléchargements tant qu'elle n'est pas réappairée.",
  pairedToast: "Extension appairée",

  // ---- pairing prompt ----
  pairTitle: "Autoriser cette extension ?",
  pairBody: (who: string) =>
    `${who} demande à envoyer ses téléchargements à TurboGrab.`,
  pairWarning:
    "En autorisant, ce client pourra ajouter des téléchargements et lire leur avancement. Tu peux révoquer l'accès dans les paramètres.",
  pairAllow: "Autoriser",
  pairDeny: "Refuser",
  unknownClient: "Un client local",

  // ---- dialogs ----
  cancelAction: "Annuler",
  confirmStopTitle: "Annuler ce téléchargement ?",
  confirmStopBody: "Le transfert en cours sera arrêté et l'élément retiré de la liste.",
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
    queued: "En file",
    downloading: "En cours",
    paused: "En pause",
    interrupted: "Interrompu",
    done: "Terminé",
    error: "Erreur",
  } as Record<string, string>,

  // Fixed backend error codes, envoyés sous la forme "@code".
  genericError: "Une erreur est survenue",
  errors: {
    download_failed: "Le téléchargement a échoué",
    link_expired: "Lien expiré",
    no_range: "Le serveur a refusé le téléchargement par segments",
    bad_link: "Lien invalide",
    busy: "Mets d'abord le téléchargement en pause",
    short_read: "Le serveur a coupé l'envoi avant la fin",
    network: "Erreur réseau — vérifie ta connexion",
    server: "Le serveur a répondu par une erreur",
    not_found: "Fichier introuvable ou supprimé",
    blocked: "Accès refusé par le serveur",
    rate_limited: "Trop de requêtes — réessaie dans quelques minutes",
    no_space: "Plus d'espace disque",
    no_write: "Écriture impossible dans ce dossier",
    no_dir: "Choisis d'abord un dossier de destination",
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
  urlPlaceholder: "Paste one or more links…",
  paste: "Paste",
  clearField: "Clear",
  download: "Download",
  destination: "Destination",
  chooseDir: "Choose a folder…",
  change: "Change",
  dirWarn: "Choose a destination folder first.",
  linkCount: (n) => `${n} links found — Shift+Enter for a new line`,
  noLink: "No http(s) link recognised.",

  interruptedTitle: (n) => `${n} interrupted download${n > 1 ? "s" : ""}`,
  resumePrompt: "Resume where it left off?",
  later: "Later",
  resumeAll: "Resume all",

  clearFinished: "Clear finished",
  search: "Filter…",
  emptyTitle: "No downloads yet.",
  emptyHint:
    "Paste a link above, or install the browser extension and your downloads land here on their own.",
  summary: (total, active) =>
    active > 0
      ? `${total} download${total > 1 ? "s" : ""} · ${active} in progress`
      : `${total} download${total > 1 ? "s" : ""}`,
  noMatch: "Nothing matches this filter.",
  clearSearch: "Clear filter",

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
  queuedHint: "Queued — starts as soon as a slot frees up.",
  viaBrowser: "from the browser",
  fileGone: "File not found on disk",

  activeSummary: (n) => `${n} download${n > 1 ? "s" : ""} in progress`,
  nothingActive: "No active downloads",

  relinkHint:
    "This link has expired. Paste an up-to-date one — what's already downloaded is kept.",
  relinkPlaceholder: "Paste the new link…",
  relinkBtn: "Resume",

  transfers: "Transfers",
  connectionsSetting: "Connections per download",
  connectionsHint:
    "Splits the file and fetches it in parallel. Set it to 1 for a server that throttles or blocks multiple requests.",
  maxActiveSetting: "Simultaneous downloads",
  maxActiveHint: "The rest wait in the queue instead of sharing the line.",

  startup: "Startup",
  autostart: "Launch on boot",
  autostartHint: "TurboGrab starts with your session, straight to the tray.",
  startHidden: "Start in the background",
  startHiddenHint: "No window on launch — the tray icon opens it when you need it.",

  browserSection: "Browser extension",
  serverEnabled: "Accept downloads from the browser",
  serverEnabledHint: "TurboGrab listens on 127.0.0.1 — this machine only, never the network.",
  serverPort: "Port",
  serverPortHint: "Must match the port set in the extension.",
  paired: "Extension paired",
  pairedHint: "The token below is what lets the extension send downloads over.",
  notPaired: "No extension paired",
  notPairedHint: "Click “Connect” in the extension: a permission prompt will appear here.",
  copyToken: "Copy token",
  makeToken: "Generate a token to paste into the extension",
  revoke: "Revoke",
  revokeBody: "The extension won't be able to send downloads until it pairs again.",
  pairedToast: "Extension paired",

  pairTitle: "Allow this extension?",
  pairBody: (who) => `${who} wants to send its downloads to TurboGrab.`,
  pairWarning:
    "Allowing this lets the client add downloads and read their progress. You can revoke it in settings.",
  pairAllow: "Allow",
  pairDeny: "Deny",
  unknownClient: "A local client",

  cancelAction: "Cancel",
  confirmStopTitle: "Cancel this download?",
  confirmStopBody: "The transfer in progress will stop and the item will leave the list.",
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
    queued: "Queued",
    downloading: "Downloading",
    paused: "Paused",
    interrupted: "Interrupted",
    done: "Done",
    error: "Error",
  },

  genericError: "Something went wrong",
  errors: {
    download_failed: "Download failed",
    link_expired: "Link expired",
    no_range: "Server refused the segmented download",
    bad_link: "Invalid link",
    busy: "Pause the download first",
    short_read: "The server cut the transfer short",
    network: "Network error — check your connection",
    server: "The server answered with an error",
    not_found: "File not found or removed",
    blocked: "The server refused access",
    rate_limited: "Too many requests — try again in a few minutes",
    no_space: "No disk space left",
    no_write: "Can't write to this folder",
    no_dir: "Choose a destination folder first",
    missing_file: "File not found on disk",
  },
};

export const DICT: Record<Lang, Dict> = { fr, en };
