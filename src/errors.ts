import type { Dict } from "./i18n";

/** Turn a backend failure code into a sentence.
 *
 *  Anything that is not an "@code" is deliberately swallowed: raw transport
 *  text is a wall of hosts, ports and nested errors, and showing it helps
 *  nobody. The real diagnostic travels in `error_detail` and the card offers to
 *  copy it. */
export function errorText(msg: string, t: Dict): string {
  return msg.startsWith("@") ? (t.errors[msg.slice(1)] ?? t.genericError) : t.genericError;
}
