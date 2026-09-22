// Markdown rendering for transcript rows (migration_plan.md §6.4, issue #15).
//
// The Godot build rendered chat rows as plain text; TKT027 asked for rich
// typography and was deferred here because the web has solved parsers. This
// is that import, with three constraints the parser does not give for free.
//
// 1. **No network egress, ever.** §2.3 makes that a product pillar, not a
//    setting. Model output is not trusted markup: a vault note, or a prompt
//    injection inside one, can put `[text](http://…)` or an `<img src>` into
//    a character's line. So links and images never become elements at all —
//    the renderer below returns their text and drops the URL before any
//    HTML exists — and DOMPurify then re-checks the result against an
//    allow-list that has no `a`, `img`, `script` or `iframe` in it. Two
//    independent layers, because one of them is a parser doing what it was
//    told and the other is a sanitiser doing what it was built for.
// 2. **Streaming stays plain.** Half-arrived markdown is broken markdown:
//    `**Bram` renders as literal asterisks until the closing pair lands, so
//    a row parsed on every delta would flicker between markup and text for
//    the length of a 15-second turn. main.ts already streams into a
//    provisional row and replaces it with the committed line, so markdown
//    is applied once, to the committed text.
// 3. **Quoted speech keeps its weight.** appendWithSpeech()'s rule — the
//    words inside quotes are what the player reads for, so they are bold —
//    predates this and is a design decision, not a markdown feature. It is
//    reapplied over the rendered tree rather than dropped.

import { marked } from "marked";
import DOMPurify from "dompurify";

// `[[Note]]` / `[[Note|label]]`: Obsidian's own syntax, which reaches the
// transcript whenever a vault note quotes another one. Shown as the words a
// person wrote, not the brackets they wrote them in. Deliberately not a
// link: nothing in the shell resolves a note title to an entity yet, and a
// styled target that does nothing when clicked is worse than prose.
const WIKI_LINK = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;

// Quoted speech inside narration: "…" or '…'. A single quote only opens
// after whitespace or a bracket and only closes before whitespace or
// punctuation, so apostrophes (You're, I'd) don't count as quotes.
// Moved here from main.ts, unchanged.
const SPEECH = /"([^"]+)"|(?<=^|[\s(])'([^']{2,}?)'(?=[\s.,;:!?)]|$)/g;

// Where re-bolding quoted speech would be wrong: inside something already
// emphasised, inside code, or inside a heading that is already heavy.
const NO_SPEECH_INSIDE = new Set(["STRONG", "EM", "CODE", "PRE", "H1", "H2", "H3", "H4"]);

// Block-level tags the transcript accepts. No `a`, `img`, `iframe`,
// `script`, `style`, `form` or `input`: see constraint 1 above. `span` is
// allowed so DOMPurify does not have to strip one the parser never emits.
const ALLOWED_TAGS = [
  "p", "br", "strong", "em", "del", "code", "pre", "blockquote",
  "ul", "ol", "li", "hr", "span",
  "h1", "h2", "h3", "h4",
  "table", "thead", "tbody", "tr", "th", "td",
];

// No attributes at all. Nothing in the rendered subset needs one, table
// alignment included, and an empty allow-list cannot be reasoned around.
const ALLOWED_ATTR: string[] = [];

marked.use({
  gfm: true,
  // A single newline is a line break. Prose written by a model uses them as
  // such; CommonMark's "two spaces or it's the same paragraph" is a rule for
  // hand-written documents and gets this wrong in a transcript.
  breaks: true,
  renderer: {
    // The URL is dropped here rather than sanitised later, so it never
    // exists as an attribute to be missed.
    link(token) {
      return this.parser.parseInline(token.tokens ?? []);
    },
    image(token) {
      return escapeHtml(token.text ?? "");
    },
    // Raw HTML in the source is shown as the characters it is, not parsed.
    // DOMPurify would strip it anyway; escaping means the player sees that
    // a note contains angle brackets instead of seeing nothing.
    html(token) {
      return escapeHtml(token.raw ?? "");
    },
  },
});

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function stripWikiLinks(text: string): string {
  return text.replace(WIKI_LINK, (_m, target: string, label?: string) =>
    (label ?? target).trim(),
  );
}

function sanitize(html: string): DocumentFragment {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    RETURN_DOM_FRAGMENT: true,
  }) as unknown as DocumentFragment;
}

/// Re-applies the quoted-speech rule across a rendered tree, in the text
/// nodes where it still makes sense. Walks a snapshot of the nodes rather
/// than the live tree, because each replacement inserts new text nodes the
/// walker would otherwise visit again.
function boldQuotedSpeech(root: DocumentFragment | HTMLElement) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const texts: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) texts.push(n as Text);

  for (const node of texts) {
    const parent = node.parentElement;
    if (parent && isInsideExemptTag(parent, root)) continue;
    const text = node.data;
    const matches = [...text.matchAll(SPEECH)];
    if (matches.length === 0) continue;

    const replacement = document.createDocumentFragment();
    let at = 0;
    for (const m of matches) {
      const start = m.index!;
      replacement.append(text.slice(at, start));
      const strong = document.createElement("strong");
      strong.textContent = m[0];
      replacement.append(strong);
      at = start + m[0].length;
    }
    replacement.append(text.slice(at));
    node.replaceWith(replacement);
  }
}

function isInsideExemptTag(from: HTMLElement, root: Node): boolean {
  for (let el: HTMLElement | null = from; el && el !== root; el = el.parentElement) {
    if (NO_SPEECH_INSIDE.has(el.tagName)) return true;
  }
  return false;
}

/// What a caller needs to know to place the result: `inline` means the
/// whole line parsed to a single paragraph, so it can go straight into the
/// row's existing `<p>` and keep every style that already applies to one.
/// `block` means the line has structure — a list, a table, a quote — that
/// cannot legally live inside a `<p>` and needs its own container.
export interface RenderedMarkdown {
  kind: "inline" | "block";
  nodes: DocumentFragment;
}

/// Renders one committed transcript line.
///
/// The single-paragraph case is not a fast path, it is the common case:
/// almost every line a character speaks is one paragraph, and unwrapping it
/// means markdown changed what the words look like without changing the
/// shape of the transcript around them.
export function renderMarkdown(text: string): RenderedMarkdown {
  const html = marked.parse(stripWikiLinks(text), { async: false }) as string;
  const fragment = sanitize(html);
  boldQuotedSpeech(fragment);

  const elements = [...fragment.children];
  if (elements.length === 1 && elements[0].tagName === "P") {
    const only = elements[0];
    const inner = document.createDocumentFragment();
    inner.append(...only.childNodes);
    return { kind: "inline", nodes: inner };
  }
  return { kind: "block", nodes: fragment };
}

/// Renders a line that must stay inline whatever it contains — a character's
/// spoken words, which sit inside a `<strong>` and are one utterance by
/// definition. Block syntax in a spoken line is a model mistake, not an
/// intention, so it is flattened rather than honoured.
export function renderInlineMarkdown(text: string): DocumentFragment {
  const html = marked.parseInline(stripWikiLinks(text), { async: false }) as string;
  const fragment = sanitize(html);
  boldQuotedSpeech(fragment);
  return fragment;
}
