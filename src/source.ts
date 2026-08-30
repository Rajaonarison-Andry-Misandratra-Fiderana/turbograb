/** What kind of transfer a pasted link needs.
 *
 *  This used to be a manual choice — two tabs the user had to pick before
 *  pasting, which also split the download list in two. The link itself already
 *  says which path it needs, so it decides, and the user only weighs in on the
 *  parts a machine can't know: video or audio, and which quality. */
export type Source = "video" | "audio" | "file";

const YT = /(^|\.)(youtube\.com|youtu\.be|youtube-nocookie\.com)$/i;

/** `null` when the text isn't a usable URL yet (still being typed). */
export function detect(url: string): "media" | "file" | null {
  const text = url.trim();
  if (!text) return null;
  try {
    // Bare "youtu.be/xyz" is a link to a human, so give the parser a scheme.
    const u = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`);
    if (!/^https?:$/.test(u.protocol)) return null;
    if (!u.hostname.includes(".")) return null;
    // Anything that isn't YouTube goes down the direct-file path, which is the
    // accelerated one. Guessing "some site yt-dlp might know" would silently
    // route real file links away from the multi-connection downloader.
    return YT.test(u.hostname) ? "media" : "file";
  } catch {
    return null;
  }
}

/** The source a fresh URL starts on, before any manual override. */
export const defaultSource = (url: string): Source =>
  detect(url) === "media" ? "video" : "file";
