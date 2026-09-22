// The knowledge graph, drawn (migration_plan.md §6.4, issue #34).
//
// §6.4 names a graph library as a thing to buy rather than build, replacing
// CampaignGraphView.gd, and this is that: Cytoscape.js, bundled, no CDN and
// no network call (§2.3).
//
// What it draws is the decision docs/design/README.md records: **the
// retrieval graph, not a world map**. Every edge here is an edge the engine
// actually traverses when it assembles a prompt. Nothing is added to make
// the picture look fuller, and node positions carry no meaning beyond "these
// are linked" — the layout is force-directed, so it says nothing about where
// anywhere is.
//
// The canvas is not the only representation. A <canvas> cannot be tabbed
// through or read aloud, and the accessibility pass (#38) is not something
// to spend on a new screen, so the map keeps a list of the same nodes and
// edges beside it as the keyboard and screen-reader path. The canvas is
// marked aria-hidden and the list is the accessible one; they are not
// redundant, they are the same graph twice for two ways of reading it.

import cytoscape from "cytoscape";

export interface GraphNodeInput {
  id: string;
  label: string;
  kind: "character" | "location" | "scene" | "lore" | "item" | "note";
  /// Present in the campaign right now. Drawn heavier: these are the people
  /// the player can actually talk to from where they are standing.
  present?: boolean;
}

export interface GraphEdgeInput {
  source: string;
  target: string;
  label: string;
}

export interface GraphHandle {
  /// Re-runs the layout and fits the view. Called after the container has
  /// been given its real size, because a layout run against a zero-width
  /// element puts every node on the same point.
  refit(): void;
  /// Dims everything that does not match, rather than hiding it: a node with
  /// no visible neighbours tells the player nothing about why it matched.
  filter(query: string): number;
  destroy(): void;
}

/// Reads a design token off the document, so the graph is themed by the same
/// custom properties as everything else and E-ink stays E-ink.
///
/// The conversion is not decoration. Cytoscape parses colour strings with its
/// own parser rather than the browser's, and that parser predates `oklch()` —
/// which every token in styles.css is written in. Handed one, it silently
/// falls back to black, which is how this first rendered: black nodes, and
/// edge labels as solid black blocks where a paper-coloured backing plate
/// should have been. Painting the token to a 1x1 canvas and reading the
/// pixel back asks the browser to do the conversion, and yields plain sRGB
/// bytes whatever colour space the token was written in.
function srgbToken(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  if (!value) return fallback;
  return toSrgbHex(value) ?? fallback;
}

let colourProbe: CanvasRenderingContext2D | null = null;

function toSrgbHex(value: string): string | null {
  if (!colourProbe) {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    colourProbe = canvas.getContext("2d", { willReadFrequently: true });
  }
  const ctx = colourProbe;
  if (!ctx) return null;
  try {
    // An unparseable value leaves fillStyle at its previous setting, so it is
    // reset to a known colour first and the fill is what proves it took.
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "#000000";
    ctx.fillStyle = value;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
    if (a === 0) return null;
    return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
  } catch {
    return null;
  }
}

export function mountGraph(
  container: HTMLElement,
  nodes: GraphNodeInput[],
  edges: GraphEdgeInput[],
  options: { onSelect?: (id: string) => void } = {},
): GraphHandle {
  const ink = srgbToken("--ink", "#3a3128");
  const ink3 = srgbToken("--ink-3", "#77706a");
  const lamp = srgbToken("--lamp", "#8a5a1e");
  const paper = srgbToken("--paper", "#f6f2e9");
  const rule = srgbToken("--rule", "#c9c2b6");

  // E-ink asks for no motion and neither does the OS setting; a graph that
  // settles over 800 ms is exactly the kind of motion both mean.
  const still =
    document.documentElement.dataset.theme === "eink" ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const cy = cytoscape({
    container,
    elements: [
      ...nodes.map((n) => ({
        data: { id: n.id, label: n.label, kind: n.kind, present: n.present ? "yes" : "no" },
      })),
      ...edges.map((e, i) => ({
        data: { id: `e${i}`, source: e.source, target: e.target, label: e.label },
      })),
    ],
    style: [
      {
        selector: "node",
        style: {
          "background-color": paper,
          "border-color": ink3,
          "border-width": 1,
          shape: "round-rectangle",
          width: "label",
          height: "label",
          padding: "8px",
          label: "data(label)",
          color: ink,
          "font-family": "Georgia, 'Times New Roman', serif",
          "font-size": "13px",
          "text-valign": "center",
          "text-halign": "center",
          "text-wrap": "wrap",
          "text-max-width": "140px",
        },
      },
      // A location is the thing the graph is mostly made of, so it stays
      // quiet; a character is who the player came here for.
      {
        selector: 'node[kind = "character"]',
        style: { "border-color": lamp, "border-width": 1.5, color: lamp },
      },
      {
        selector: 'node[present = "yes"]',
        style: { "border-width": 2.5, "font-weight": "bold" },
      },
      {
        selector: "edge",
        style: {
          width: 1,
          "line-color": rule,
          "target-arrow-color": rule,
          "target-arrow-shape": "triangle",
          "arrow-scale": 0.7,
          "curve-style": "bezier",
          label: "data(label)",
          color: ink3,
          "font-family": "'IBM Plex Mono', ui-monospace, monospace",
          "font-size": "9px",
          "text-rotation": "autorotate",
          "text-background-color": paper,
          "text-background-opacity": 1,
          "text-background-padding": "2px",
        },
      },
      { selector: ".dim", style: { opacity: 0.15 } },
      {
        selector: ".match",
        style: { "border-color": lamp, "border-width": 3, color: lamp },
      },
      {
        selector: "node:selected",
        style: { "background-color": ink, color: paper, "border-color": ink },
      },
    ],
    // Zoom bounds: far enough out to see a large vault whole, not so far in
    // that a label fills the panel.
    minZoom: 0.2,
    maxZoom: 2.5,
    wheelSensitivity: 0.2,
  });

  const layout = () =>
    cy
      .layout({
        name: "cose",
        animate: !still,
        animationDuration: 700,
        // Long enough that linked notes read as linked without the graph
        // becoming a hairball on a vault with a few hundred of them.
        // Lay out inside the panel rather than wherever the forces settle.
        // Without this, cose spreads a handful of nodes across an area far
        // wider than the container and the fit that follows zooms the whole
        // graph out until no label can be read — which is exactly what
        // raising repulsion to stop nodes overlapping caused. Bounding the
        // layout fixes both ends: nodes separate, and they separate into the
        // space that exists.
        boundingBox: { x1: 0, y1: 0, w: container.clientWidth, h: container.clientHeight },
        idealEdgeLength: () => 110,
        nodeOverlap: 20,
        nodeRepulsion: () => 9000,
        // An unlinked note is a real finding — "nothing points at this yet" —
        // so a disconnected component is kept clear of the main cluster
        // rather than sitting in its margins looking like part of it.
        componentSpacing: 80,
        padding: 24,
        randomize: true,
        fit: true,
      })
      .run();

  layout();

  // A small graph laid out compactly can fit at 2x or more, which renders a
  // seven-node vault as seven billboards. Fitting only ever shrinks from
  // here; it never blows the graph up past its natural size.
  cy.ready(() => {
    if (cy.zoom() > 1) cy.zoom({ level: 1, position: { x: cy.width() / 2, y: cy.height() / 2 } });
  });

  if (options.onSelect) {
    cy.on("tap", "node", (event) => options.onSelect!(event.target.id() as string));
  }

  return {
    refit: () => {
      cy.resize();
      layout();
    },
    filter: (query: string) => {
      const q = query.trim().toLowerCase();
      cy.elements().removeClass("dim match");
      if (!q) return cy.nodes().length;
      const matched = cy
        .nodes()
        .filter((n) => String(n.data("label")).toLowerCase().includes(q));
      if (matched.length === 0) return 0;
      // Keep each match's immediate neighbourhood lit: a matched node alone
      // in the dark does not say why it matched.
      const keep = matched.closedNeighborhood();
      cy.elements().difference(keep).addClass("dim");
      matched.addClass("match");
      return matched.length;
    },
    destroy: () => cy.destroy(),
  };
}
