// Tiny FR/EN dictionary. `t = DICT[lang]`; a few entries are functions for
// count-dependent strings.
export type Lang = "fr" | "en";

export interface Dict {
  tagline: string;
  tabFile: string;
  later: string;
  resumeAll: string;
  resumePrompt: string;
  interruptedTitle: (n: number) => string;
  ytPlaceholder: string;
  filePlaceholder: string;
  video: string;
  audio: string;
  fetch: string;
  download: string;
  chooseDir: string;
  change: string;
  dirWarn: string;
  itemsLabel: (n: number) => string;
  clearFinished: string;
  emptyTitle: string;
  emptyFile: string;
  emptyYt: string;
  genericError: string;
  best: string;
  retry: string;
  pause: string;
  resume: string;
  delete: string;
  askDeleteFile: string;
  askDeleteFilesBulk: string;
  status: Record<string, string>;
}

export const DICT: Record<Lang, Dict> = {
  fr: {
    tagline: "Téléchargeur — YouTube & fichiers",
    tabFile: "Fichier",
    later: "Plus tard",
    resumeAll: "Tout reprendre",
    resumePrompt: "Reprendre là où ça s'est arrêté ?",
    interruptedTitle: (n) =>
      `${n} téléchargement${n > 1 ? "s" : ""} interrompu${n > 1 ? "s" : ""}`,
    ytPlaceholder: "Colle une URL YouTube…",
    filePlaceholder: "Colle un lien de fichier direct…",
    video: "Vidéo",
    audio: "Audio",
    fetch: "Récupérer",
    download: "Télécharger",
    chooseDir: "Choisir un dossier…",
    change: "Changer",
    dirWarn: "Choisis d'abord un dossier de destination.",
    itemsLabel: (n) => `${n} élément${n > 1 ? "s" : ""}`,
    clearFinished: "Effacer les terminés",
    emptyTitle: "Aucun téléchargement pour l'instant.",
    emptyFile: "Colle un lien de fichier direct ci-dessus.",
    emptyYt: "Colle une URL YouTube ci-dessus pour commencer.",
    genericError: "Une erreur est survenue",
    best: "Meilleure",
    retry: "Réessayer",
    pause: "Pause",
    resume: "Reprendre",
    delete: "Supprimer",
    askDeleteFile: "Supprimer aussi le fichier du disque ?",
    askDeleteFilesBulk: "Supprimer aussi les fichiers téléchargés du disque ?",
    status: {
      fetching: "Analyse…",
      ready: "Prêt",
      downloading: "En cours",
      paused: "En pause",
      interrupted: "Interrompu",
      done: "Terminé",
      error: "Erreur",
    },
  },
  en: {
    tagline: "Downloader — YouTube & files",
    tabFile: "File",
    later: "Later",
    resumeAll: "Resume all",
    resumePrompt: "Resume where it left off?",
    interruptedTitle: (n) => `${n} interrupted download${n > 1 ? "s" : ""}`,
    ytPlaceholder: "Paste a YouTube URL…",
    filePlaceholder: "Paste a direct file link…",
    video: "Video",
    audio: "Audio",
    fetch: "Fetch",
    download: "Download",
    chooseDir: "Choose a folder…",
    change: "Change",
    dirWarn: "Choose a destination folder first.",
    itemsLabel: (n) => `${n} item${n > 1 ? "s" : ""}`,
    clearFinished: "Clear finished",
    emptyTitle: "No downloads yet.",
    emptyFile: "Paste a direct file link above.",
    emptyYt: "Paste a YouTube URL above to start.",
    genericError: "Something went wrong",
    best: "Best",
    retry: "Retry",
    pause: "Pause",
    resume: "Resume",
    delete: "Delete",
    askDeleteFile: "Also delete the file from disk?",
    askDeleteFilesBulk: "Also delete the downloaded files from disk?",
    status: {
      fetching: "Analyzing…",
      ready: "Ready",
      downloading: "Downloading",
      paused: "Paused",
      interrupted: "Interrupted",
      done: "Done",
      error: "Error",
    },
  },
};
