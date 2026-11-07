// Companion reply formatting (decision 36). Replies are plain text except fenced code blocks and
// http(s) links. Model output is untrusted: everything here is pure string work, and the result is
// rendered as React text nodes only (never as HTML).

export type Segment =
  | { kind: "text"; text: string }
  | { kind: "code"; lang: string | null; code: string; closed: boolean };

export type LinkPart = { kind: "text"; text: string } | { kind: "link"; url: string; text: string };

/** Longest language tag kept from a fence's info string. */
const LANG_MAX = 20;
/** Longest URL that is linked; longer ones stay plain text. */
export const MAX_URL_LENGTH = 2048;

// Fence rules (a subset of CommonMark): a fence line starts with up to 3 spaces, then 3 or more
// backticks or tildes ("```" or "~~~"), then an optional info string. A backtick fence's info
// string may not contain a backtick. A closing fence uses the same character, at least as many
// of them, and nothing else on the line. A fence that never closes is a code block with
// closed:false, so a reply that is still streaming does not flicker into plain text.
const OPEN_FENCE = /^ {0,3}(`{3,}|~{3,})[ \t]*(.*)$/;
const CLOSE_FENCE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/;

/** First word of the info string, lowercased and reduced to [a-z0-9#+._-]; null when empty. */
function cleanLang(info: string): string | null {
  const first = info.trim().split(/\s+/, 1)[0] ?? "";
  const lang = first
    .toLowerCase()
    .replace(/[^a-z0-9#+._-]/g, "")
    .slice(0, LANG_MAX);
  return lang === "" ? null : lang;
}

/** Splits a reply into text and fenced-code segments in one linear pass. */
export function parseReply(text: string): Segment[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const out: Segment[] = [];
  let buf: string[] = [];
  let fence: { char: string; len: number; lang: string | null } | null = null;

  const flushText = () => {
    if (buf.length > 0) {
      const joined = buf.join("\n");
      if (joined !== "") out.push({ kind: "text", text: joined });
    }
    buf = [];
  };

  for (const line of lines) {
    if (fence === null) {
      const m = OPEN_FENCE.exec(line);
      const marker = m?.[1];
      const info = m?.[2] ?? "";
      if (m && marker && !(marker.startsWith("`") && info.includes("`"))) {
        flushText();
        fence = { char: marker[0] ?? "`", len: marker.length, lang: cleanLang(info) };
      } else {
        buf.push(line);
      }
    } else {
      const m = CLOSE_FENCE.exec(line);
      const marker = m?.[1];
      if (marker && marker[0] === fence.char && marker.length >= fence.len) {
        out.push({ kind: "code", lang: fence.lang, code: buf.join("\n"), closed: true });
        buf = [];
        fence = null;
      } else {
        buf.push(line);
      }
    }
  }
  if (fence !== null) {
    out.push({ kind: "code", lang: fence.lang, code: buf.join("\n"), closed: false });
  } else {
    flushText();
  }
  return out;
}

// A candidate starts with http:// or https:// (any case), not glued to a preceding word.
const URL_CANDIDATE = /(?<![A-Za-z0-9+.-])https?:\/\/[^\s<>]+/gi;
const TRAILING = new Set([".", ",", ";", ":", "!", "?", ")", "]", "}", "'", '"', "”", "’", "»"]);
const PAIRS: Record<string, string> = { ")": "(", "]": "[", "}": "{" };

function count(s: string, ch: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) if (s[i] === ch) n++;
  return n;
}

/** Drops trailing punctuation, keeping a closing bracket that has its opener in the URL. */
function trimTrailing(candidate: string): string {
  let end = candidate.length;
  while (end > 0) {
    const ch = candidate[end - 1] ?? "";
    if (!TRAILING.has(ch)) break;
    const open = PAIRS[ch];
    if (open) {
      const head = candidate.slice(0, end);
      if (count(head, ch) <= count(head, open)) break;
    }
    end--;
  }
  return candidate.slice(0, end);
}

/**
 * A URL is linked only if it is http or https, parses, has a host, fits MAX_URL_LENGTH and has no
 * embedded credentials. A URL with credentials ("https://user:pw@host") is NOT linked: it stays
 * plain text, because the part before "@" is a common way to disguise the real host.
 */
function linkable(url: string): boolean {
  if (url.length > MAX_URL_LENGTH) return false;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;
  if (parsed.hostname === "") return false;
  if (parsed.username !== "" || parsed.password !== "") return false;
  return true;
}

/** Finds http(s) links in plain text. Nothing else is ever a link. */
export function findLinks(text: string): LinkPart[] {
  const out: LinkPart[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_CANDIDATE)) {
    const start = m.index;
    const url = trimTrailing(m[0]);
    if (!linkable(url)) continue;
    if (start > last) out.push({ kind: "text", text: text.slice(last, start) });
    out.push({ kind: "link", url, text: url });
    last = start + url.length;
  }
  if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
  return out;
}

/**
 * The URL as the browser will request it (WHATWG serialization): internationalised hosts show as
 * ASCII punycode, so a look-alike host cannot pass for another. Falls back to the input.
 */
export function displayUrl(url: string): string {
  try {
    return new URL(url).href;
  } catch {
    return url;
  }
}

/** The ASCII host name of a URL, or an empty string when it does not parse. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}
