// A page that mounts the real graph module with fixture data, so the
// knowledge graph can be looked at without a compiled vault, a running
// Ollama, or the Tauri shell around it. See ../README.md for why looking
// matters here. Not built, not shipped, not in CI.

// The app's own stylesheet, not a copy: the graph reads its colours from
// these design tokens, so a preview with its own palette would prove nothing.
import "../../src/styles.css";
import { mountGraph } from "../../src/graph";
const nodes = [
  { id: "elara", label: "Elara Voss", kind: "character" as const, present: true },
  { id: "bram", label: "Bram Holt", kind: "character" as const, present: false },
  { id: "mill", label: "The Old Mill", kind: "location" as const },
  { id: "road", label: "Mill Road", kind: "location" as const },
  { id: "chapel", label: "Pale Reach Chapel", kind: "location" as const },
  { id: "accord", label: "The Quillion Accord", kind: "note" as const },
  { id: "lonely", label: "Unlinked Note", kind: "note" as const },
];
const edges = [
  { source: "mill", target: "road", label: "connected to" },
  { source: "road", target: "chapel", label: "connected to" },
  { source: "elara", target: "mill", label: "associated with" },
  { source: "bram", target: "mill", label: "links to" },
  { source: "accord", target: "chapel", label: "mentions" },
];
// The theme is chosen before mount, exactly as the map screen does it:
// applyTheme() sets the attribute, then renderMap() builds a fresh graph.
const theme = new URLSearchParams(location.search).get("theme");
if (theme) document.documentElement.dataset.theme = theme;
const handle = mountGraph(document.querySelector("#graph")!, nodes, edges, {
  onSelect: (id) => ((window as any).__selected = id),
});
(window as any).__graph = handle;
(window as any).__ready = true;
