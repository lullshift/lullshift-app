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

// A candidate starts with http:// or https:// (any case), not glued to a preceding word. The
// regex itself is linear: each match is one greedy run of non-space characters.
const URL_CANDIDATE = /(?<![A-Za-z0-9+.-])https?:\/\/[^\s<>]+/gi;
const TRAILING = new Set([".", ",", ";", ":", "!", "?", ")", "]", "}", "'", '"', "”", "’", "»"]);
const PAIRS: Record<string, string> = { ")": "(", "]": "[", "}": "{" };
/**
 * Trailing punctuation allowed after a URL of MAX_URL_LENGTH ("…link).", "…link)?!"). A longer
 * candidate can't become a linkable URL, so it is skipped before any trimming (security review F1:
 * trimming used to be quadratic and ran on candidates of any length).
 */
const MAX_TRAILING = 8;

/**
 * Drops trailing punctuation, keeping a closing bracket that has its opener in the URL. Bracket
 * counts are taken once and updated as characters are dropped, so this is linear.
 */
function trimTrailing(candidate: string): string {
  const counts: Record<string, number> = { "(": 0, ")": 0, "[": 0, "]": 0, "{": 0, "}": 0 };
  for (let i = 0; i < candidate.length; i++) {
    const ch = candidate[i] ?? "";
    if (ch in counts) counts[ch] = (counts[ch] ?? 0) + 1;
  }
  let end = candidate.length;
  while (end > 0) {
    const ch = candidate[end - 1] ?? "";
    if (!TRAILING.has(ch)) break;
    const open = PAIRS[ch];
    if (open) {
      if ((counts[ch] ?? 0) <= (counts[open] ?? 0)) break;
      counts[ch] = (counts[ch] ?? 0) - 1;
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
    if (m[0].length > MAX_URL_LENGTH + MAX_TRAILING) continue;
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

const LOCAL_SUFFIXES = [".localhost", ".local", ".lan", ".internal", ".home.arpa"];

function ipv4Parts(host: string): number[] | null {
  const parts = host.split(".");
  if (parts.length !== 4 || !parts.every((p) => /^\d{1,3}$/.test(p))) return null;
  const n = parts.map(Number);
  return n.every((x) => x <= 255) ? n : null;
}

function privateIpv4([a, b]: number[]): boolean {
  if (a === undefined || b === undefined) return false;
  return (
    a === 0 || // "this network", 0.0.0.0 reaches this computer
    a === 10 ||
    a === 127 ||
    (a === 100 && b >= 64 && b <= 127) || // carrier-grade NAT
    (a === 169 && b === 254) || // link-local
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168)
  );
}

/**
 * True when a URL's host is this computer or the local network: localhost and loopback, private,
 * link-local and unique-local addresses, local-only names (.local, .lan, .internal, .home.arpa)
 * and single-label names such as "router". Opening such a link sends a request to a service on
 * the user's own computer or network (security review F6), so the confirmation says so. Takes a
 * URL as `displayUrl` returns it; the browser's own parser has already turned numeric forms such
 * as "http://2130706433" into dotted IPv4.
 */
export function isLocalHost(url: string): boolean {
  let host: string;
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return false;
  }
  if (host === "") return false;
  if (host.startsWith("[") && host.endsWith("]")) {
    const v6 = host.slice(1, -1);
    if (v6 === "::1" || v6 === "::") return true;
    const mapped = /^::ffff:(\d+\.\d+\.\d+\.\d+)$/.exec(v6)?.[1];
    if (mapped) {
      const parts = ipv4Parts(mapped);
      return parts !== null && privateIpv4(parts);
    }
    // WHATWG writes a mapped address in hex ("::ffff:7f00:1"): first hextet of the IPv4 part.
    const hexMapped = /^::ffff:([0-9a-f]{1,4}):[0-9a-f]{1,4}$/.exec(v6)?.[1];
    if (hexMapped) {
      const v = parseInt(hexMapped, 16);
      return privateIpv4([v >> 8, v & 0xff]);
    }
    const first = parseInt(v6.split(":")[0] || "0", 16);
    return (first & 0xfe00) === 0xfc00 || (first & 0xffc0) === 0xfe80;
  }
  const v4 = ipv4Parts(host);
  if (v4) return privateIpv4(v4);
  const name = host.endsWith(".") ? host.slice(0, -1) : host;
  if (name === "localhost" || !name.includes(".")) return true;
  return LOCAL_SUFFIXES.some((s) => name.endsWith(s));
}

/** The ASCII host name of a URL, or an empty string when it does not parse. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}
