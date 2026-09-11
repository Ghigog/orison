// Orison desktop shell (migration_plan.md §6.1, docs/design/Orison.dc.html).
//
// A small vanilla-TS router over the command layer in
// apps/desktop/src-tauri/src/commands.rs. Every screen here either renders
// real data from a real command, or says plainly that it doesn't have real
// data yet — the product's whole premise is "no estimate is ever shown"
// (docs/design/README.md), and a UI that fakes numbers to look finished
// argues against the thing it's shipping.
//
// Wired to real commands: Campaigns, Import, Models, Play (streamed events
// plus a turn-outcome instrument strip), and a read-only slice of Character
// and Map (whatever EntityDto carries). Not wired: rapport/emotion history
// and knowledge-graph edges — no command exposes them yet, so those sections
// say so instead of inventing numbers.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// Types mirroring the Rust side. TurnEvent/TurnState/DirectorState/
// FailureKind are serde's default externally-tagged representation: a
// fieldless variant is its bare name as a string, a variant with fields is
// `{ "VariantName": { ...fields } }`.

interface CampaignSummary {
  id: string;
  title: string;
  last_played: string;
  playtime_seconds: number;
  active_scene: string;
}

interface EntityDto {
  id: string;
  label: string;
  kind: "character" | "location" | "scene" | "lore" | "item" | "note";
  description: string;
}

interface IngestReportDto {
  notes_seen: number;
  sections_mapped: number;
  sections_overflowed: number;
  chunks: number;
  notes_without_a_type: number;
  unaccounted_sections: string[];
  dangling_link_count: number;
  gender_conflict_count: number;
}

interface ModelsSummaryDto {
  summary: string;
  context_length: number;
  tokenizer_is_approximate: boolean;
}

type ModelHealthStatus =
  | { kind: "available" }
  | { kind: "modelNotInstalled" }
  | { kind: "unreachable"; detail: string };

interface ModelHealthDto {
  model: string;
  status: ModelHealthStatus;
  contextLength: number | null;
}

type Speaker = "Player" | { Character: string } | "Narrator" | "System";

type TurnEvent =
  | { StateChanged: "Idle" | "Preparing" | "Streaming" | "Applying" }
  | { DirectorStateChanged: "Idle" | "Researching" | "Composing" | "Ready" }
  | { PlayerMessage: { text: string } }
  | { StreamStarted: { speaker: Speaker; field: string } }
  | { StreamDelta: { text: string } }
  | { StreamEnded: { field: string } }
  | { Message: { speaker: Speaker; text: string } }
  | { LocationChanged: { entity_id: string; label: string } }
  | { SystemMessage: { text: string } }
  | { Failed: { kind: string; detail: string } }
  | "TurnCompleted";

interface TurnOutcome {
  latencyMs: number;
  timeToFirstTokenMs: number | null;
  promptTokens: number;
  evaluatedPromptTokens: number | null;
  promptEvalTimeMs: number | null;
  completionTokens: number;
  retrieved: number;
  directorTriggered: boolean;
}

// ---------------------------------------------------------------------------
// App shell: a nav rail plus one active screen, matching
// docs/design/Orison.dc.html's SESSION/DESK grouping. Screens without a
// backing command yet are still listed — clicking them says why rather than
// pretending the nav item isn't there.

type Screen =
  | { name: "campaigns" }
  | { name: "connect"; campaignId: string; campaignTitle: string }
  | { name: "play"; campaignId: string; campaignTitle: string }
  | { name: "character"; campaignId: string; entity: EntityDto }
  | { name: "map"; campaignId: string }
  | { name: "import" }
  | { name: "settings" };

let current: Screen = { name: "campaigns" };
let lastPlayContext: { campaignId: string; campaignTitle: string } | null = null;

const app = document.querySelector<HTMLDivElement>("#app")!;

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

function shell(railActive: string, body: string): string {
  const item = (id: string, label: string) =>
    `<button class="nav-item${id === railActive ? " active" : ""}" data-nav="${id}">
      <span class="mark">${id === railActive ? "●" : "·"}</span><span>${label}</span>
    </button>`;
  return `
    <nav class="rail">
      <div class="brand mono">ORISON</div>
      <div class="brand-sub mono">local · offline · yours</div>
      <div class="nav-group mono">SESSION</div>
      ${lastPlayContext ? item("play", "Play") : ""}
      ${lastPlayContext ? item("map", "What it knows") : ""}
      <div class="nav-group mono">DESK</div>
      ${item("campaigns", "Campaigns")}
      ${item("import", "Compile a vault")}
      ${item("settings", "Settings")}
    </nav>
    <main class="screen">${body}</main>
  `;
}

function attachNav() {
  app.querySelectorAll<HTMLButtonElement>("[data-nav]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.nav!;
      if (id === "play" && lastPlayContext) {
        render({ name: "play", ...lastPlayContext });
      } else if (id === "map" && lastPlayContext) {
        render({ name: "map", campaignId: lastPlayContext.campaignId });
      } else if (id === "campaigns") {
        render({ name: "campaigns" });
      } else if (id === "import") {
        render({ name: "import" });
      } else if (id === "settings") {
        render({ name: "settings" });
      }
    });
  });
}

async function render(screen: Screen) {
  current = screen;
  switch (screen.name) {
    case "campaigns":
      return renderCampaigns();
    case "connect":
      return renderConnect(screen.campaignId, screen.campaignTitle);
    case "play":
      lastPlayContext = { campaignId: screen.campaignId, campaignTitle: screen.campaignTitle };
      return renderPlay(screen.campaignId, screen.campaignTitle);
    case "character":
      return renderCharacter(screen.campaignId, screen.entity);
    case "map":
      return renderMap(screen.campaignId);
    case "import":
      return renderImport();
    case "settings":
      return renderSettings();
  }
}

// ---------------------------------------------------------------------------
// Campaigns (docs/design/Orison.dc.html "library")

async function renderCampaigns() {
  let campaigns: CampaignSummary[] = [];
  let error = "";
  try {
    campaigns = await invoke<CampaignSummary[]>("list_campaigns");
  } catch (e) {
    error = String(e);
  }

  app.innerHTML = shell(
    "campaigns",
    `
    <div class="pad">
      <h1>Pick up where you left off</h1>
      ${error ? `<p class="error mono">${escapeHtml(error)}</p>` : ""}
      ${
        campaigns.length === 0 && !error
          ? `<p class="mono muted">No campaigns yet. Create one with \`orison new\` or the vault-compile screen.</p>`
          : campaigns
              .map(
                (c) => `
        <div class="row">
          <div>
            <div class="title-lg">${escapeHtml(c.title)}</div>
            <div class="mono meta">${escapeHtml(c.id)} · last played ${escapeHtml(c.last_played)} · ${Math.round(c.playtime_seconds / 60)} min</div>
          </div>
          <button class="btn" data-resume="${escapeHtml(c.id)}" data-title="${escapeHtml(c.title)}">RESUME</button>
        </div>`,
              )
              .join("")
      }
    </div>
  `,
  );
  attachNav();
  app.querySelectorAll<HTMLButtonElement>("[data-resume]").forEach((btn) => {
    btn.addEventListener("click", () =>
      render({ name: "connect", campaignId: btn.dataset.resume!, campaignTitle: btn.dataset.title! }),
    );
  });
}

// ---------------------------------------------------------------------------
// Models / connect (docs/design/Orison.dc.html "models", trimmed to what
// connect_models actually needs)

/// Renders a `check_model_health` result into a `● REACHABLE` / `● NOT
/// PULLED` / `● UNREACHABLE` badge, matching docs/design/Orison.dc.html's
/// "models" screen — the two "endpoint answered" failure modes are kept
/// visually distinct per #29's acceptance criteria, not collapsed into one
/// generic error.
function renderHealthBadge(badge: HTMLElement, detail: HTMLElement, health: ModelHealthDto) {
  switch (health.status.kind) {
    case "available":
      badge.textContent = "● REACHABLE";
      badge.className = "mono health-badge ok";
      detail.textContent = "";
      break;
    case "modelNotInstalled":
      badge.textContent = "● NOT PULLED";
      badge.className = "mono health-badge warn";
      detail.textContent = `ollama pull ${health.model}`;
      break;
    case "unreachable":
      badge.textContent = "● UNREACHABLE";
      badge.className = "mono health-badge warn";
      detail.textContent = health.status.detail;
      break;
  }
}

/// Wires one model field's blur (never keystroke — a 15-second-turn model
/// deserves a debounce, not a request per character) to `check_model_health`.
/// `requestId` guards against an in-flight check for a since-edited value
/// landing after a newer one and overwriting it with stale badge text.
function attachHealthCheck(input: HTMLInputElement, urlInput: HTMLInputElement, badge: HTMLElement, detail: HTMLElement) {
  let requestId = 0;
  const run = async () => {
    const model = input.value.trim();
    if (!model) {
      badge.textContent = "";
      detail.textContent = "";
      return;
    }
    const thisRequest = ++requestId;
    badge.textContent = "checking…";
    badge.className = "mono health-badge";
    detail.textContent = "";
    try {
      const health = await invoke<ModelHealthDto>("check_model_health", {
        url: urlInput.value.trim(),
        model,
      });
      if (thisRequest === requestId) renderHealthBadge(badge, detail, health);
    } catch (e) {
      if (thisRequest === requestId) {
        badge.textContent = "● CHECK FAILED";
        badge.className = "mono health-badge warn";
        detail.textContent = String(e);
      }
    }
  };
  input.addEventListener("blur", run);
  urlInput.addEventListener("blur", run);
}

async function renderConnect(campaignId: string, campaignTitle: string) {
  app.innerHTML = shell(
    "campaigns",
    `
    <div class="pad">
      <h1>Two minds, both on this machine</h1>
      <p class="muted">Connecting to <b>${escapeHtml(campaignTitle)}</b>. One decides what happens next; one speaks.</p>
      <div class="field">
        <label class="mono">ENDPOINT</label>
        <input id="url" value="http://127.0.0.1:11434" />
      </div>
      <div class="field">
        <div style="display:flex;justify-content:space-between;align-items:baseline">
          <label class="mono">THE ACTOR — speaks in character</label>
          <span id="actor-health" class="mono health-badge"></span>
        </div>
        <input id="actor-model" value="llama3.2:3b" />
        <div id="actor-health-detail" class="mono health-detail"></div>
      </div>
      <div class="field">
        <div style="display:flex;justify-content:space-between;align-items:baseline">
          <label class="mono">THE DIRECTOR — composes what happens next (blank = same as the Actor)</label>
          <span id="director-health" class="mono health-badge"></span>
        </div>
        <input id="director-model" placeholder="llama3.2:3b" />
        <div id="director-health-detail" class="mono health-detail"></div>
      </div>
      <label class="mono checkbox"><input type="checkbox" id="two-calls" /> Director + Actor (two calls; unchecked runs a single call)</label>
      <div style="margin-top:20px">
        <button class="btn btn-solid" id="connect">CONNECT</button>
      </div>
      <p id="connect-error" class="error mono"></p>
    </div>
  `,
  );
  attachNav();

  const urlInput = document.querySelector<HTMLInputElement>("#url")!;
  const actorInput = document.querySelector<HTMLInputElement>("#actor-model")!;
  const directorInput = document.querySelector<HTMLInputElement>("#director-model")!;
  attachHealthCheck(actorInput, urlInput, document.querySelector("#actor-health")!, document.querySelector("#actor-health-detail")!);
  attachHealthCheck(directorInput, urlInput, document.querySelector("#director-health")!, document.querySelector("#director-health-detail")!);

  document.querySelector("#connect")!.addEventListener("click", async () => {
    const url = urlInput.value;
    const actorModel = actorInput.value;
    const directorModelRaw = directorInput.value.trim();
    const twoCalls = (document.querySelector("#two-calls") as HTMLInputElement).checked;
    try {
      const summary = await invoke<ModelsSummaryDto>("connect_models", {
        campaignId,
        args: {
          url,
          actorModel,
          directorModel: directorModelRaw || null,
          twoCalls,
          contextLimit: null,
        },
      });
      console.log("connected:", summary.summary);
      render({ name: "play", campaignId, campaignTitle });
    } catch (e) {
      document.querySelector("#connect-error")!.textContent = String(e);
    }
  });
}

// ---------------------------------------------------------------------------
// Play (docs/design/Orison.dc.html "play") — the screen the whole design
// direction is built around. Streams TurnEvent, shows the real instrument
// strip from "turn-outcome" once a turn lands, and renders a Failed event as
// an inline interrupted state rather than a toast (round 1's decision,
// carried into round 2's Settings "Instrumented" level).

let playListenersAttached = false;

async function renderPlay(campaignId: string, campaignTitle: string) {
  app.innerHTML = shell(
    "play",
    `
    <div class="play-header">
      <div>
        <div class="title-lg" id="location-label">${escapeHtml(campaignTitle)}</div>
        <div class="mono meta" id="director-indicator">DIRECTOR · IDLE</div>
      </div>
    </div>
    <div class="sheet">
      <div id="transcript" class="transcript"></div>
      <div id="instrument-strip" class="instrument mono"></div>
      <div class="composer">
        <span class="mono lamp">&rsaquo;</span>
        <input id="draft" placeholder="say something, or type / for commands" autofocus />
      </div>
    </div>
  `,
  );
  attachNav();

  const transcript = document.querySelector<HTMLDivElement>("#transcript")!;
  const instrumentStrip = document.querySelector<HTMLDivElement>("#instrument-strip")!;
  const directorIndicator = document.querySelector<HTMLDivElement>("#director-indicator")!;
  const locationLabel = document.querySelector<HTMLDivElement>("#location-label")!;
  const draft = document.querySelector<HTMLInputElement>("#draft")!;

  // listen() stacks a new handler on every render; this screen is only ever
  // shown for one campaign at a time in this scaffold, so guard rather than
  // unlisten on navigate-away (a real router would track the unlisten fn).
  if (!playListenersAttached) {
    playListenersAttached = true;
    await listen<{ campaignId: string; event: TurnEvent }>("turn-event", (e) => {
      if (e.payload.campaignId !== currentPlayCampaignId()) return;
      handleTurnEvent(e.payload.event, transcript, directorIndicator, locationLabel);
    });
    await listen<{ campaignId: string; outcome: TurnOutcome }>("turn-outcome", (e) => {
      if (e.payload.campaignId !== currentPlayCampaignId()) return;
      renderInstrumentStrip(instrumentStrip, e.payload.outcome);
    });
  }

  draft.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || !draft.value.trim()) return;
    const text = draft.value;
    draft.value = "";
    await invoke("submit_player_input", { campaignId, text }).catch((err) =>
      appendLine(transcript, "ERROR", String(err), "error"),
    );
  });
}

function currentPlayCampaignId(): string | undefined {
  return current.name === "play" ? current.campaignId : undefined;
}

function handleTurnEvent(
  event: TurnEvent,
  transcript: HTMLDivElement,
  directorIndicator: HTMLDivElement,
  locationLabel: HTMLDivElement,
) {
  if (typeof event === "string") {
    return; // TurnCompleted: the outcome event carries what's worth showing.
  }
  if ("DirectorStateChanged" in event) {
    directorIndicator.textContent = `DIRECTOR · ${event.DirectorStateChanged.toUpperCase()}`;
    return;
  }
  if ("LocationChanged" in event) {
    locationLabel.textContent = event.LocationChanged.label;
    return;
  }
  if ("StreamDelta" in event) {
    const last = transcript.lastElementChild;
    if (last?.classList.contains("streaming")) {
      last.textContent += event.StreamDelta.text;
    } else {
      appendLine(transcript, "", event.StreamDelta.text, "streaming");
    }
    return;
  }
  if ("Message" in event) {
    const last = transcript.lastElementChild;
    if (last?.classList.contains("streaming")) last.remove(); // replaced by the parsed line
    appendLine(transcript, speakerLabel(event.Message.speaker), event.Message.text);
    return;
  }
  if ("SystemMessage" in event) {
    appendLine(transcript, "SYSTEM", event.SystemMessage.text, "system");
    return;
  }
  if ("Failed" in event) {
    appendFailure(transcript, event.Failed.kind, event.Failed.detail);
    return;
  }
}

function speakerLabel(speaker: Speaker): string {
  if (speaker === "Player") return "YOU";
  if (speaker === "Narrator") return "";
  if (speaker === "System") return "SYSTEM";
  return speaker.Character.toUpperCase();
}

function appendLine(transcript: HTMLDivElement, label: string, text: string, cls = "") {
  const p = document.createElement("p");
  p.className = cls;
  p.textContent = label ? `${label}: ${text}` : text;
  transcript.appendChild(p);
  transcript.scrollTop = transcript.scrollHeight;
}

// The Interrupted screen's decision (docs/design/Orison.dc.html "failure"),
// inline rather than a modal: nothing already committed is lost, so this
// renders in place of a toast, with the three real exits — retry, check
// models, or accept what already streamed.
function appendFailure(transcript: HTMLDivElement, kind: string, detail: string) {
  const div = document.createElement("div");
  div.className = "interrupted";
  div.innerHTML = `
    <div class="mono lamp">FAILED · ${escapeHtml(kind)}</div>
    <p>${escapeHtml(detail)}</p>
    <p class="muted">Nothing already committed was lost. Retrying re-asks rather than repeats.</p>
  `;
  transcript.appendChild(div);
  transcript.scrollTop = transcript.scrollHeight;
}

function renderInstrumentStrip(el: HTMLDivElement, outcome: TurnOutcome) {
  const parts = [
    outcome.timeToFirstTokenMs != null ? `ttft ${(outcome.timeToFirstTokenMs / 1000).toFixed(1)} s` : null,
    `${outcome.completionTokens} tokens out`,
    outcome.evaluatedPromptTokens != null
      ? `${outcome.evaluatedPromptTokens} of ${outcome.promptTokens} prompt tokens evaluated`
      : `${outcome.promptTokens} prompt tokens (approximate)`,
    `${outcome.retrieved} passages retrieved`,
    outcome.directorTriggered ? "Director triggered" : null,
    `${(outcome.latencyMs / 1000).toFixed(1)} s total`,
  ].filter(Boolean);
  el.textContent = parts.join(" · ");
}

// ---------------------------------------------------------------------------
// Character (partial — docs/design/Orison.dc.html "character" also shows
// rapport and an emotion-event log; no command exposes EmotionEngine state
// yet, so this section says so rather than inventing numbers.)

async function renderCharacter(_campaignId: string, entity: EntityDto) {
  app.innerHTML = shell(
    "play",
    `
    <div class="pad">
      <div class="mono meta">CHARACTER</div>
      <h1>${escapeHtml(entity.label)}</h1>
      <div class="sheet-block">
        <div class="mono meta">FROM THE VAULT</div>
        <p>${escapeHtml(entity.description) || "<em>No description on file.</em>"}</p>
      </div>
      <div class="sheet-block">
        <div class="mono meta">RAPPORT, AND WHAT SHE FELT AND WHY</div>
        <p class="muted">Not wired yet — no command exposes per-character emotion state.
        See docs/design/Orison.dc.html for the target shape.</p>
      </div>
    </div>
  `,
  );
  attachNav();
}

// ---------------------------------------------------------------------------
// Map (partial — real entities from `locations`, no graph edges yet: no
// command exposes the knowledge graph's structure, only its nodes.)

async function renderMap(campaignId: string) {
  let locations: EntityDto[] = [];
  let characters: EntityDto[] = [];
  let error = "";
  try {
    [locations, characters] = await Promise.all([
      invoke<EntityDto[]>("locations", { campaignId }),
      invoke<EntityDto[]>("characters_present", { campaignId }),
    ]);
  } catch (e) {
    error = String(e);
  }
  const row = (e: EntityDto, clickable: boolean) => `
    <div class="row"${clickable ? ` data-entity="${escapeHtml(e.id)}"` : ""} style="${clickable ? "cursor:pointer" : ""}">
      <div><div class="title-lg">${escapeHtml(e.label)}</div>
      <p class="muted">${escapeHtml(e.description)}</p></div>
    </div>`;
  app.innerHTML = shell(
    "map",
    `
    <div class="pad">
      <div class="mono meta">KNOWLEDGE GRAPH</div>
      <h1>What this campaign knows</h1>
      ${error ? `<p class="error mono">${escapeHtml(error)}</p>` : ""}
      <p class="muted">Nodes only — no command exposes the graph's edges yet, so this
      lists entities rather than drawing the retrieval graph docs/design/Orison.dc.html shows.</p>
      <div class="mono meta" style="margin-top:24px">PRESENT NOW</div>
      ${characters.map((c) => row(c, true)).join("") || `<p class="muted">Nobody here.</p>`}
      <div class="mono meta" style="margin-top:24px">LOCATIONS</div>
      ${locations.map((l) => row(l, false)).join("")}
    </div>
  `,
  );
  attachNav();
  app.querySelectorAll<HTMLDivElement>("[data-entity]").forEach((el) => {
    const entity = characters.find((c) => c.id === el.dataset.entity);
    if (entity) el.addEventListener("click", () => render({ name: "character", campaignId, entity }));
  });
}

// ---------------------------------------------------------------------------
// Import (docs/design/Orison.dc.html "import", the folder-type mapping
// screen simplified to one free-text mapping field for this scaffold)

async function renderImport() {
  app.innerHTML = shell(
    "import",
    `
    <div class="pad">
      <h1>Compile a vault</h1>
      <div class="field">
        <label class="mono">CAMPAIGN TITLE (creates a new campaign)</label>
        <input id="title" placeholder="Thornwick" />
      </div>
      <div class="field">
        <label class="mono">VAULT PATH</label>
        <input id="vault" placeholder="fixtures/vaults/minimal" />
      </div>
      <div class="field">
        <label class="mono">FOLDER TYPES — one per line, folder=type (type is character/location/lore/item/ignore)</label>
        <textarea id="folder-types" rows="4" placeholder="People=character&#10;Places/Cities=location"></textarea>
      </div>
      <button class="btn btn-solid" id="compile">COMPILE</button>
      <p id="import-error" class="error mono"></p>
      <pre id="import-report" class="mono"></pre>
    </div>
  `,
  );
  attachNav();
  document.querySelector("#compile")!.addEventListener("click", async () => {
    const title = (document.querySelector("#title") as HTMLInputElement).value.trim();
    const vault = (document.querySelector("#vault") as HTMLInputElement).value.trim();
    const folderTypesRaw = (document.querySelector("#folder-types") as HTMLTextAreaElement).value;
    const folderTypes: Record<string, string> = {};
    for (const line of folderTypesRaw.split("\n")) {
      const [folder, kind] = line.split("=").map((s) => s.trim());
      if (folder && kind) folderTypes[folder] = kind;
    }
    try {
      const created = await invoke<CampaignSummary>("create_campaign", {
        title,
        vault: vault || null,
        folderTypes,
      });
      const report = await invoke<IngestReportDto>("import_vault", {
        campaignId: created.id,
        vault,
        folderTypes,
      });
      document.querySelector("#import-report")!.textContent = JSON.stringify(report, null, 2);
    } catch (e) {
      document.querySelector("#import-error")!.textContent = String(e);
    }
  });
}

// ---------------------------------------------------------------------------
// Settings (docs/design/Orison.dc.html "settings" — the theme/verbosity
// toggles are view-only local state here; nothing persists them yet.)

function renderSettings() {
  app.innerHTML = shell(
    "settings",
    `
    <div class="pad">
      <h1>The room you read in</h1>
      <div class="sheet-block">
        <div class="mono meta">WHAT LEAVES THIS MACHINE</div>
        <p style="font-size:20px;margin:0">Nothing.</p>
        <p class="muted">Orison reaches one address, and you typed it. No telemetry, no crash reports.</p>
      </div>
      <p class="muted">Theme and verbosity toggles (Lamp/E-ink, Instrumented/Narrative/Quiet) are
      drawn in docs/design/Orison.dc.html but not wired to this shell yet — no
      settings command exists to persist them.</p>
    </div>
  `,
  );
  attachNav();
}

render({ name: "campaigns" });
