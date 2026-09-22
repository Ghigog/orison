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
// plus a turn-outcome instrument strip), Character (vault description plus
// live rapport and current emotion), and Map (nodes plus the graph's real
// edges, #34). Not wired: the per-turn "what she felt, and why" log — no
// command exposes it yet, so that section says so instead of inventing
// numbers.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open } from "@tauri-apps/plugin-dialog";

import { renderInlineMarkdown, renderMarkdown } from "./markdown";
// Types only: graph.ts pulls in Cytoscape, which is ~440 kB of the bundle
// and is needed by exactly one screen. It is imported for real inside
// renderMap() so the other seven screens never parse it.
import type { GraphEdgeInput, GraphHandle, GraphNodeInput } from "./graph";

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

interface EdgeDto {
  fromId: string;
  fromLabel: string;
  toId: string;
  toLabel: string;
  kind: string;
}

interface CharacterEmotionDto {
  emotion: string;
  intensity: number;
  target: string;
  reason: string;
  affinity: number;
  rapportLabel: string;
  rapportBehaviour: string;
}

interface IngestReportDto {
  notesSeen: number;
  sectionsMapped: number;
  sectionsOverflowed: number;
  chunks: number;
  notesWithoutAType: number;
  unaccountedSections: string[];
  danglingLinkCount: number;
  genderConflictCount: number;
}

interface ModelsSummaryDto {
  summary: string;
  context_length: number;
  tokenizer_is_approximate: boolean;
}

interface ModelArgsDto {
  url: string;
  actorModel: string;
  directorModel: string | null;
  twoCalls: boolean;
  contextLimit: number | null;
}

interface StarterDto {
  title: string;
  description: string;
  locationId: string;
  characterId: string;
  narration: string;
}

type StarterProgress =
  | { campaignId: string; stage: "building_clusters" | "selecting_hooks"; index: null; total: null }
  | { campaignId: string; stage: "writing_narration"; index: number; total: number };

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

interface VaultFolder {
  folder: string;
  notes: number;
  guess: string;
}

interface HistoryLine {
  role: "player" | "character" | "narrator" | "system";
  text: string;
  sender: string | null;
}

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
  | { name: "starters"; campaignId: string; campaignTitle: string; modelArgs: ModelArgsDto }
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
    <nav class="rail" aria-label="Primary">
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
  // Leaving the map detaches its container but not the Cytoscape instance
  // behind it, which keeps a canvas and its own window listeners alive.
  if (current.name === "map" && screen.name !== "map") {
    activeGraph?.destroy();
    activeGraph = null;
  }
  current = screen;
  switch (screen.name) {
    case "campaigns":
      return renderCampaigns();
    case "connect":
      return renderConnect(screen.campaignId, screen.campaignTitle);
    case "starters":
      return renderStarters(screen.campaignId, screen.campaignTitle, screen.modelArgs);
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
          <div style="display:flex;gap:8px">
            <button class="btn" data-resume="${escapeHtml(c.id)}" data-title="${escapeHtml(c.title)}">RESUME</button>
            <button class="btn" data-delete="${escapeHtml(c.id)}" data-title="${escapeHtml(c.title)}">DELETE</button>
          </div>
        </div>`,
              )
              .join("")
      }
    </div>
  `,
  );
  attachNav();
  app.querySelectorAll<HTMLButtonElement>("[data-delete]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const campaignId = btn.dataset.delete!;
      const yes = await ask(
        `Delete "${btn.dataset.title}" from Orison? Its story so far, memories and compiled notes are removed. Your vault files are not touched, so you can compile them again later.`,
        { title: "Delete campaign", kind: "warning", okLabel: "Delete", cancelLabel: "Keep" },
      );
      if (!yes) return;
      try {
        await invoke("delete_campaign", { campaignId });
        if (lastPlayContext?.campaignId === campaignId) lastPlayContext = null;
        await renderCampaigns();
      } catch (e) {
        alert(String(e));
      }
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-resume]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const campaignId = btn.dataset.resume!;
      const campaignTitle = btn.dataset.title!;
      const connected = await invoke<boolean>("is_connected", { campaignId }).catch(() => false);
      render({ name: connected ? "play" : "connect", campaignId, campaignTitle });
    });
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

// The last connection that worked, so the connect screen opens on it. Not
// per-campaign: the models are a property of the machine, not the story.
const LAST_MODELS_KEY = "orison.lastModels";
interface SavedModels {
  url: string;
  actorModel: string;
  directorModel: string;
  twoCalls: boolean;
}

function loadSavedModels(): SavedModels | null {
  try {
    return JSON.parse(localStorage.getItem(LAST_MODELS_KEY) ?? "null");
  } catch {
    return null;
  }
}

function saveModels(m: SavedModels) {
  try {
    localStorage.setItem(LAST_MODELS_KEY, JSON.stringify(m));
  } catch {
    // Storage unavailable: the next connect just starts from the defaults.
  }
}

async function renderConnect(campaignId: string, campaignTitle: string) {
  const saved = loadSavedModels();
  app.innerHTML = shell(
    "campaigns",
    `
    <div class="pad">
      <h1>Two minds, both on this machine</h1>
      <p class="muted">Connecting to <b>${escapeHtml(campaignTitle)}</b>. One decides what happens next; one speaks.</p>
      <div class="field">
        <label class="mono" for="url">ENDPOINT</label>
        <input id="url" value="${escapeHtml(saved?.url ?? "http://127.0.0.1:11434")}" />
      </div>
      <div class="field">
        <div style="display:flex;justify-content:space-between;align-items:baseline">
          <label class="mono" for="actor-model">THE ACTOR — speaks in character</label>
          <span id="actor-health" class="mono health-badge"></span>
        </div>
        <input id="actor-model" value="${escapeHtml(saved?.actorModel ?? "llama3.2:3b")}" />
        <div id="actor-health-detail" class="mono health-detail"></div>
      </div>
      <div class="field">
        <div style="display:flex;justify-content:space-between;align-items:baseline">
          <label class="mono" for="director-model">THE DIRECTOR — composes what happens next (blank = same as the Actor)</label>
          <span id="director-health" class="mono health-badge"></span>
        </div>
        <input id="director-model" placeholder="llama3.2:3b" value="${escapeHtml(saved?.directorModel ?? "")}" />
        <div id="director-health-detail" class="mono health-detail"></div>
      </div>
      <label class="mono checkbox"><input type="checkbox" id="two-calls"${saved?.twoCalls ? " checked" : ""} /> Director + Actor (two calls; unchecked runs a single call)</label>
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
      saveModels({ url, actorModel, directorModel: directorModelRaw, twoCalls });
      render({
        name: "starters",
        campaignId,
        campaignTitle,
        modelArgs: {
          url,
          actorModel,
          directorModel: directorModelRaw || null,
          twoCalls,
          contextLimit: null,
        },
      });
    } catch (e) {
      document.querySelector("#connect-error")!.textContent = String(e);
    }
  });
}

// ---------------------------------------------------------------------------
// Adventure starters: between compile and play, the two-pass pipeline
// (`orison_core::onboarding`) proposes up to 3 opening hooks and the player
// picks one, or skips straight to `import`'s deterministic default start.

let startersListenerAttached = false;

async function renderStarters(campaignId: string, campaignTitle: string, modelArgs: ModelArgsDto) {
  app.innerHTML = shell(
    "campaigns",
    `
    <div class="pad">
      <h1>How does it begin?</h1>
      <p class="muted">Writing a few ways into <b>${escapeHtml(campaignTitle)}</b>…</p>
      <p id="starters-progress" class="mono muted"></p>
      <div id="starters-list"></div>
      <p id="starters-error" class="error mono"></p>
      <div style="margin-top:20px">
        <button class="btn" id="starters-skip">SKIP — JUST START</button>
      </div>
    </div>
  `,
  );
  attachNav();

  if (!startersListenerAttached) {
    startersListenerAttached = true;
    await listen<StarterProgress>("starter-progress", (e) => {
      if (current.name !== "starters" || current.campaignId !== e.payload.campaignId) return;
      const progressLine = document.querySelector<HTMLParagraphElement>("#starters-progress");
      if (!progressLine) return;
      switch (e.payload.stage) {
        case "building_clusters":
          progressLine.textContent = "Finding starting places…";
          break;
        case "selecting_hooks":
          progressLine.textContent = "Choosing hooks…";
          break;
        case "writing_narration":
          progressLine.textContent = `Writing hook ${e.payload.index + 1}/${e.payload.total}…`;
          break;
      }
    });
  }

  document.querySelector("#starters-skip")!.addEventListener("click", () => {
    render({ name: "play", campaignId, campaignTitle });
  });

  let starters: StarterDto[];
  try {
    starters = await invoke<StarterDto[]>("generate_starters", { campaignId, args: modelArgs });
  } catch (e) {
    if (current.name === "starters" && current.campaignId === campaignId) {
      document.querySelector("#starters-error")!.textContent = String(e);
    }
    return;
  }
  if (current.name !== "starters" || current.campaignId !== campaignId) return; // navigated away mid-generation

  document.querySelector<HTMLParagraphElement>("#starters-progress")!.textContent = "";
  document.querySelector("#starters-list")!.innerHTML = starters
    .map(
      (s, i) => `
    <div class="row">
      <div>
        <div class="title-lg">${escapeHtml(s.title)}</div>
        <div class="mono meta">${escapeHtml(s.description)}</div>
        <p>${escapeHtml(s.narration)}</p>
      </div>
      <button class="btn btn-solid" data-pick="${i}">BEGIN HERE</button>
    </div>`,
    )
    .join("");
  app.querySelectorAll<HTMLButtonElement>("[data-pick]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const starter = starters[Number(btn.dataset.pick)];
      try {
        await invoke("pick_starter", { campaignId, starter });
        render({ name: "play", campaignId, campaignTitle });
      } catch (e) {
        document.querySelector("#starters-error")!.textContent = String(e);
      }
    });
  });
}

// ---------------------------------------------------------------------------
// Play (docs/design/Orison.dc.html "play") — the screen the whole design
// direction is built around. Streams TurnEvent, shows the real instrument
// strip from "turn-outcome" once a turn lands, and renders a Failed event as
// an inline interrupted state rather than a toast (round 1's decision,
// carried into round 2's Settings "Instrumented" level).

let playListenersAttached = false;

// Command grammar, mirrored from `orison_cli::shell::parse_command` so the
// desktop composer has the same `/who`, `/talk`, `/where`, `/go`, `/status`,
// `/save`, `/quit` vocabulary as `orison-cli`'s `Shell` rather than a
// second, drifting implementation of it.
type PlayCommand =
  | { kind: "help" }
  | { kind: "who" }
  | { kind: "talk"; name: string }
  | { kind: "where" }
  | { kind: "go"; name: string }
  | { kind: "status" }
  | { kind: "save" }
  | { kind: "quit" }
  | { kind: "say"; text: string }
  | { kind: "unknown"; message: string };

function parsePlayCommand(line: string): PlayCommand {
  const trimmed = line.trim();
  if (!trimmed.startsWith("/")) return { kind: "say", text: trimmed };
  const rest = trimmed.slice(1);
  const spaceAt = rest.search(/\s/);
  const word = (spaceAt === -1 ? rest : rest.slice(0, spaceAt)).toLowerCase();
  const argument = (spaceAt === -1 ? "" : rest.slice(spaceAt + 1)).trim();
  const need = (what: string): { name: string } | { kind: "unknown"; message: string } =>
    argument
      ? { name: argument }
      : { kind: "unknown", message: `/${word} needs ${what}, for example: /${word} <name>` };
  switch (word) {
    case "help":
    case "?":
      return { kind: "help" };
    case "who":
      return { kind: "who" };
    case "talk":
    case "speak": {
      const r = need("somebody to talk to");
      return "kind" in r ? r : { kind: "talk", name: r.name };
    }
    case "where":
    case "look":
      return { kind: "where" };
    case "go":
    case "travel": {
      const r = need("somewhere to go");
      return "kind" in r ? r : { kind: "go", name: r.name };
    }
    case "status":
    case "state":
      return { kind: "status" };
    case "save":
      return { kind: "save" };
    case "quit":
    case "exit":
      return { kind: "quit" };
    default:
      return {
        kind: "unknown",
        message: `no command called /${word}. /help lists them. To say it out loud, drop the slash.`,
      };
  }
}

const PLAY_HELP_LINES = [
  "/who              who is here to talk to",
  "/talk <name>      address somebody",
  "/where            where you are, and what leads away",
  "/go <place>       travel there",
  "/status           what the engine is doing",
  "/save             every turn is already saved",
  "/quit             leave this campaign",
  "anything else     is said out loud to whoever you are addressing",
];

/// Runs everything that is not `Say` or `Quit` — those stay in the draft's
/// `keydown` handler, which is what already knows how to submit a turn and
/// how to leave. `move_to_location` and `select_character` are the same
/// `TurnEngine` calls `shell.rs`'s `go`/`talk` make; `move_to_location`
/// already emits `TurnEvent::LocationChanged`, which `handleTurnEvent`
/// already renders, so a successful `/go` needs no line appended here.
async function dispatchPlayCommand(
  command: PlayCommand,
  transcript: HTMLDivElement,
  campaignId: string,
) {
  const line = (text: string) => appendLine(transcript, "", text, "system");
  switch (command.kind) {
    case "help":
      for (const l of PLAY_HELP_LINES) line(l);
      return;
    case "who": {
      const present = await invoke<EntityDto[]>("characters_present", { campaignId }).catch(
        (e) => {
          line(String(e));
          return null;
        },
      );
      if (!present) return;
      if (present.length === 0) {
        line("Nobody here. /where, then /go somewhere with people in it.");
        return;
      }
      line("Here:");
      for (const e of present) line(`  ${e.label}`);
      return;
    }
    case "talk": {
      const present = await invoke<EntityDto[]>("characters_present", { campaignId }).catch(
        () => [] as EntityDto[],
      );
      const match = present.find((e) => e.label.toLowerCase() === command.name.toLowerCase());
      if (!match) {
        line(`Nobody here is called "${command.name}". /who lists them.`);
        return;
      }
      await invoke("select_character", { campaignId, entityId: match.id }).catch((e) =>
        line(String(e)),
      );
      line(`You turn to ${match.label}.`);
      return;
    }
    case "where": {
      const [here, exits] = await Promise.all([
        invoke<EntityDto | null>("current_location", { campaignId }).catch(() => null),
        invoke<EntityDto[]>("exits", { campaignId }).catch(() => [] as EntityDto[]),
      ]);
      line(here ? here.label : "Nowhere in particular yet.");
      if (exits.length === 0) line("  Nothing leads away from here.");
      else {
        line("  From here:");
        for (const e of exits) line(`    ${e.label}`);
      }
      return;
    }
    case "go":
      try {
        await invoke<EntityDto>("move_to_location", { campaignId, name: command.name });
      } catch (e) {
        line(`You cannot: ${e}.`);
      }
      return;
    case "status":
      line(
        `turn ${turnState} | director ${directorState} | ${turnState === "Idle" ? "ready for input" : "busy"}`,
      );
      return;
    case "save":
      line("Saved. (Every turn is already committed.)");
      return;
    case "quit":
      render({ name: "campaigns" });
      return;
    case "unknown":
      line(command.message);
      return;
  }
}

async function renderPlay(campaignId: string, campaignTitle: string) {
  app.innerHTML = shell(
    "play",
    `
    <div class="play-screen">
      <div class="play-header">
        <div>
          <div class="title-lg" id="location-label">${escapeHtml(campaignTitle)}</div>
          <div class="mono meta" id="director-indicator" aria-live="polite">DIRECTOR · IDLE</div>
        </div>
      </div>
      <div class="sheet">
        <div id="transcript" class="transcript" role="log" aria-live="polite" aria-label="Story so far"></div>
        <div id="instrument-strip" class="instrument mono"></div>
        <div class="composer">
          <span class="mono lamp" aria-hidden="true">&rsaquo;</span>
          <input id="draft" placeholder="say something, or type / for commands" aria-label="Say something, or type / for commands" autofocus />
        </div>
        <div class="composer-hint mono muted">
          /who · /talk &lt;name&gt; · /where · /go &lt;place&gt; · /status · /save · /quit — esc to stop a turn
        </div>
      </div>
    </div>
  `,
  );
  attachNav();

  const transcript = document.querySelector<HTMLDivElement>("#transcript")!;
  const draft = document.querySelector<HTMLInputElement>("#draft")!;

  // The screen is rebuilt on every visit; the transcript lives in the store.
  const history = await invoke<HistoryLine[]>("recent_history", { campaignId, limit: 40 }).catch(
    () => [] as HistoryLine[],
  );
  for (const line of history) {
    if (line.role === "player") appendLine(transcript, "YOU", line.text, "player");
    else if (line.role === "system") appendLine(transcript, "SYSTEM", line.text, "system");
    else if (line.role === "character") appendSpeech(transcript, line.sender ?? "", line.text);
    else appendLine(transcript, "", line.text);
  }

  // listen() stacks a new handler on every render, so it is attached once.
  // That means the handlers outlive this render's DOM: they must look the
  // elements up when an event arrives, not close over the ones from the
  // first render (which are detached after any navigate-away-and-back).
  if (!playListenersAttached) {
    playListenersAttached = true;
    // Esc stops a turn that's taking too long, matching
    // docs/design/Orison.dc.html's instrument-strip hint. Bound to the
    // document rather than the draft input so it still works while a turn
    // is streaming and the player hasn't clicked back into the composer.
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape" || current.name !== "play") return;
      if (turnState !== "Preparing" && turnState !== "Streaming") return;
      e.preventDefault();
      void invoke("cancel_current_turn", { campaignId: current.campaignId }).catch(() => {});
    });
    await listen<{ campaignId: string; event: TurnEvent }>("turn-event", (e) => {
      if (e.payload.campaignId !== currentPlayCampaignId()) return;
      const q = <T extends HTMLElement>(sel: string) => document.querySelector<T>(sel);
      const transcript = q<HTMLDivElement>("#transcript");
      const indicator = q<HTMLDivElement>("#director-indicator");
      const location = q<HTMLDivElement>("#location-label");
      if (!transcript || !indicator || !location) return;
      handleTurnEvent(e.payload.event, transcript, indicator, location);
    });
    await listen<{ campaignId: string; outcome: TurnOutcome }>("turn-outcome", (e) => {
      if (e.payload.campaignId !== currentPlayCampaignId()) return;
      const strip = document.querySelector<HTMLDivElement>("#instrument-strip");
      if (strip) renderInstrumentStrip(strip, e.payload.outcome);
    });
  }

  draft.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || !draft.value.trim()) return;
    const text = draft.value;
    draft.value = "";
    const command = parsePlayCommand(text);
    if (command.kind === "say") {
      await invoke("submit_player_input", { campaignId, text: command.text }).catch((err) =>
        appendLine(transcript, "ERROR", String(err), "error"),
      );
      return;
    }
    await dispatchPlayCommand(command, transcript, campaignId);
  });
}

function currentPlayCampaignId(): string | undefined {
  return current.name === "play" ? current.campaignId : undefined;
}

// The header line answers "is it working?" — without the turn state the
// player's only signal during a 10-30 s turn was silence.
let turnState = "Idle";
// Where the current stream zone's text is going; null between zones.
let streamTarget: HTMLElement | null = null;
let directorState = "Idle";

// The turn machine's own names, in words a player can read. "Applying" is
// the stretch after the reply has landed while state is written back — the
// text is on screen but the turn is not over, which "STREAMING" got wrong.
const TURN_LABEL: Record<string, string> = {
  Preparing: "RECALLING",
  Streaming: "WRITING",
  Applying: "SETTLING",
};

function paintStatus(el: HTMLDivElement) {
  const turn = TURN_LABEL[turnState];
  el.textContent = turn
    ? `${turn}… · DIRECTOR · ${directorState.toUpperCase()}`
    : `DIRECTOR · ${directorState.toUpperCase()}`;
  el.classList.toggle("busy", !!turn);
}

/// A row that says the turn is under way until the first text replaces it.
function showThinking(transcript: HTMLDivElement) {
  if (transcript.querySelector(".thinking")) return;
  const p = document.createElement("p");
  p.className = "thinking";
  p.textContent = "…";
  transcript.appendChild(p);
  transcript.scrollTop = transcript.scrollHeight;
}

function clearThinking(transcript: HTMLDivElement) {
  transcript.querySelector(".thinking")?.remove();
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
  if ("StateChanged" in event) {
    turnState = event.StateChanged;
    paintStatus(directorIndicator);
    if (turnState === "Preparing") showThinking(transcript);
    if (turnState === "Idle") clearThinking(transcript);
    return;
  }
  if ("DirectorStateChanged" in event) {
    directorState = event.DirectorStateChanged;
    paintStatus(directorIndicator);
    return;
  }
  if ("PlayerMessage" in event) {
    appendLine(transcript, "YOU", event.PlayerMessage.text, "player");
    return;
  }
  if ("LocationChanged" in event) {
    locationLabel.textContent = event.LocationChanged.label;
    return;
  }
  if ("StreamStarted" in event) {
    // One row per field: the narration and the character's speech arrive as
    // separate zones and must not be run together into one paragraph.
    clearThinking(transcript);
    const { speaker } = event.StreamStarted;
    const p = document.createElement("p");
    p.className = "streaming";
    if (typeof speaker === "object") {
      p.classList.add("speech");
      p.append(`${speaker.Character.toUpperCase()}: `);
      streamTarget = document.createElement("strong");
      p.append(streamTarget);
    } else {
      streamTarget = p;
    }
    transcript.appendChild(p);
    return;
  }
  if ("StreamDelta" in event) {
    clearThinking(transcript);
    streamTarget?.append(event.StreamDelta.text);
    transcript.scrollTop = transcript.scrollHeight;
    return;
  }
  if ("StreamEnded" in event) {
    streamTarget = null;
    return;
  }
  if ("Message" in event) {
    clearThinking(transcript);
    const { speaker, text } = event.Message;
    // The parsed line takes the place of its provisional stream row, so
    // nothing jumps when the response finishes.
    const provisional = transcript.querySelector(".streaming");
    if (typeof speaker === "object") {
      appendSpeech(transcript, speaker.Character, text);
    } else {
      appendLine(transcript, speakerLabel(speaker), text);
    }
    if (provisional) provisional.replaceWith(transcript.lastElementChild!);
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

// The label is its own span, not a bare text node, so the small-frame
// breakpoint (styles.css) can move it onto its own line above the text
// instead of the desktop's inline "LABEL: text" — docs/design/Orison.dc.html
// "mobile"'s "speaker names move above the line instead of a left gutter"
// (#37); this app never built that gutter, so this is what moves.
function appendLabel(p: HTMLElement, label: string) {
  const span = document.createElement("span");
  span.className = "line-label";
  span.textContent = `${label}: `;
  p.append(span);
}

// Where markdown is applied, and where it is not (issue #15). A row with no
// class is narration — prose a model wrote — and is rendered. SYSTEM, YOU and
// ERROR rows are engine output, the player's own typed words, and a Rust
// error string respectively: none of those is authored prose, and rendering
// them would mean the shell silently rewrote text somebody expects to see
// exactly as they sent it. markdown.ts carries the rest of the reasoning.
function appendLine(transcript: HTMLDivElement, label: string, text: string, cls = "") {
  const prose = cls === "" ? renderMarkdown(text) : null;
  // Structure — a list, a table, a quote — cannot live inside a <p>, so a
  // row that has any gets a <div> instead. One paragraph, which is nearly
  // every line, keeps the <p> and every style already written against it.
  const row = document.createElement(prose?.kind === "block" ? "div" : "p");
  row.className = prose?.kind === "block" ? "md" : cls;
  if (label) appendLabel(row, label);
  row.append(prose ? prose.nodes : text);
  transcript.appendChild(row);
  transcript.scrollTop = transcript.scrollHeight;
}

/// A character's line, in bold: speech is the part the player reads for.
/// Rendered inline whatever it contains — a spoken line is one utterance by
/// definition, so block syntax in it is a model slip, not an intention.
function appendSpeech(transcript: HTMLDivElement, speaker: string, text: string) {
  const p = document.createElement("p");
  p.className = "speech";
  appendLabel(p, speaker.toUpperCase());
  const strong = document.createElement("strong");
  strong.append(renderInlineMarkdown(text));
  p.append(strong);
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
// Character (partial — docs/design/Orison.dc.html "character" also shows a
// per-turn "what she felt, and why" event log; there is no persisted history
// of past emotion events yet, only this one live snapshot, so that section
// says so rather than inventing entries. #33.)

async function renderCharacter(campaignId: string, entity: EntityDto) {
  let emotion: CharacterEmotionDto | null = null;
  let error = "";
  try {
    emotion = await invoke<CharacterEmotionDto>("character_emotion", {
      campaignId,
      entityId: entity.id,
    });
  } catch (e) {
    error = String(e);
  }

  const rapportBlock = error
    ? `<p class="error mono">${escapeHtml(error)}</p>`
    : emotion
      ? `
      <div class="rapport-head">
        <span class="rapport-label">${escapeHtml(emotion.rapportLabel)}</span>
        <span class="mono rapport-score">${emotion.affinity >= 0 ? "+" : ""}${emotion.affinity.toFixed(2)}</span>
      </div>
      <div class="rapport-meter">
        <div class="rapport-meter-mark" style="left:${(((emotion.affinity + 1) / 2) * 100).toFixed(1)}%"></div>
      </div>
      <div class="rapport-scale mono">
        <span>NEMESIS −1</span><span>0</span><span>+1 DEVOTED</span>
      </div>
      <p class="rapport-behaviour">${escapeHtml(emotion.rapportBehaviour)}</p>
    `
      : "";

  const feelingBlock =
    !error && emotion
      ? `
      <div class="mono meta" style="margin-top:18px">CURRENT FEELING</div>
      <p class="capitalize">${escapeHtml(emotion.emotion)} (intensity ${emotion.intensity.toFixed(1)}/1.0) toward ${escapeHtml(emotion.target)}${
        emotion.reason ? ` — ${escapeHtml(emotion.reason)}` : ""
      }</p>
      <p class="muted">What she felt on past turns isn't tracked yet — this is the current
      state only.</p>
    `
      : "";

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
        <div class="mono meta">RAPPORT</div>
        ${rapportBlock}
        ${feelingBlock}
      </div>
    </div>
  `,
  );
  attachNav();
}

// ---------------------------------------------------------------------------
// Map: real entities from `locations`/`characters_present`, plus the graph's
// real edges from `graph_edges` (#34) — "what it knows" means what
// docs/design/Orison.dc.html says it means, the graph the engine actually
// traverses, not a flat node list. RAPTOR summary nodes are excluded on the
// Rust side (`TurnEngine::graph_edges`), and a dangling link — named but
// never written — cannot be told apart from "no such edge" once the graph is
// loaded, since `KnowledgeGraph::connect` never stores one in the first
// place; that distinction only exists in the import report (#36).

/// `connected_to` -> "connected to". An author's own `relationships:`
/// wording (e.g. "sworn enemy of") passes through unchanged; it never
/// contains underscores to begin with.
function edgeVerb(kind: string): string {
  return kind.replace(/_/g, " ");
}

async function renderMap(campaignId: string) {
  let locations: EntityDto[] = [];
  let characters: EntityDto[] = [];
  let edges: EdgeDto[] = [];
  let error = "";
  try {
    [locations, characters, edges] = await Promise.all([
      invoke<EntityDto[]>("locations", { campaignId }),
      invoke<EntityDto[]>("characters_present", { campaignId }),
      invoke<EdgeDto[]>("graph_edges", { campaignId }),
    ]);
  } catch (e) {
    error = String(e);
  }

  // Nodes come from the edges as well as from the entity lists, because the
  // graph reaches things neither list returns — lore notes, items, a RAPTOR
  // summary. Dropping an endpoint because it is not a location or a present
  // character would draw an edge to nothing, or quietly hide a hop the
  // engine really does make. Known entities keep their kind and description;
  // an endpoint nothing else describes is a note.
  const known = new Map<string, EntityDto>();
  for (const e of [...locations, ...characters]) known.set(e.id, e);
  const present = new Set(characters.map((c) => c.id));

  const graphNodes = new Map<string, GraphNodeInput>();
  const addNode = (id: string, label: string) => {
    if (graphNodes.has(id)) return;
    const entity = known.get(id);
    graphNodes.set(id, {
      id,
      label: entity?.label ?? label,
      kind: entity?.kind ?? "note",
      present: present.has(id),
    });
  };
  for (const e of edges) {
    addNode(e.fromId, e.fromLabel);
    addNode(e.toId, e.toLabel);
  }
  // An entity with no edges at all is still something the campaign knows, and
  // "nothing links to this yet" is a finding rather than a reason to hide it.
  for (const e of [...locations, ...characters]) addNode(e.id, e.label);

  const nodes = [...graphNodes.values()];
  const graphEdges: GraphEdgeInput[] = edges.map((e) => ({
    source: e.fromId,
    target: e.toId,
    label: edgeVerb(e.kind),
  }));

  // A present character is a real <button> so it's reachable and
  // activatable by keyboard, not just a click target (#38); a location row
  // has nothing to activate, so it stays a plain <div>.
  const row = (e: EntityDto, clickable: boolean) => {
    const tag = clickable ? "button" : "div";
    return `
    <${tag} class="row"${clickable ? ` type="button" data-entity="${escapeHtml(e.id)}"` : ""}>
      <div><div class="title-lg">${escapeHtml(e.label)}</div>
      <p class="muted">${escapeHtml(e.description)}</p></div>
    </${tag}>`;
  };
  const edgeRow = (e: EdgeDto) => `
    <div class="row mono">
      ${escapeHtml(e.fromLabel)} —${escapeHtml(edgeVerb(e.kind))}→ ${escapeHtml(e.toLabel)}
    </div>`;

  app.innerHTML = shell(
    "map",
    `
    <div class="pad">
      <div class="mono meta">KNOWLEDGE GRAPH</div>
      <h1>What this campaign knows</h1>
      <p class="muted">
        Every line is a link the engine really follows when it builds a prompt —
        a wiki-link, a frontmatter connection, or one note naming another in its
        prose. Where things sit means nothing; what they join to does.
      </p>
      ${error ? `<p class="error mono">${escapeHtml(error)}</p>` : ""}
      <div class="graph-toolbar">
        <input id="graph-search" type="search" placeholder="find a note, a person, a place"
               aria-label="Filter the graph" ${nodes.length ? "" : "disabled"} />
        <button class="btn" id="graph-refit" type="button" ${nodes.length ? "" : "disabled"}>REDRAW</button>
        <span class="mono muted" id="graph-count">${nodes.length} nodes · ${edges.length} links</span>
      </div>
      ${
        nodes.length
          ? `<div id="graph" class="graph" aria-hidden="true"></div>`
          : `<p class="muted">Nothing is linked to anything yet. Compile a vault to give it something to know.</p>`
      }
      <div class="mono meta" style="margin-top:24px">PRESENT NOW</div>
      ${characters.map((c) => row(c, true)).join("") || `<p class="muted">Nobody here.</p>`}
      <div class="mono meta" style="margin-top:24px">LOCATIONS</div>
      ${locations.map((l) => row(l, false)).join("")}
      <div class="mono meta" style="margin-top:24px">CONNECTIONS</div>
      <p class="muted">The same links as the drawing above, in a form you can tab through and a screen reader can read.</p>
      ${edges.map(edgeRow).join("") || `<p class="muted">Nothing links to anything yet.</p>`}
    </div>
  `,
  );
  attachNav();

  const openCharacter = (id: string) => {
    const entity = characters.find((c) => c.id === id);
    if (entity) render({ name: "character", campaignId, entity });
  };
  app.querySelectorAll<HTMLButtonElement>("[data-entity]").forEach((el) => {
    el.addEventListener("click", () => openCharacter(el.dataset.entity!));
  });

  const container = document.querySelector<HTMLDivElement>("#graph");
  if (!container) return;

  // Torn down and rebuilt on every visit, like the rest of the screen. It
  // holds a canvas and its own event listeners, so leaving the old one
  // attached to a detached container leaks both.
  activeGraph?.destroy();
  const { mountGraph } = await import("./graph");
  // Awaiting the import gives the router time to move on; mounting into a
  // container that is no longer the screen would leave an orphan canvas.
  if (current.name !== "map" || !container.isConnected) return;
  activeGraph = mountGraph(container, nodes, graphEdges, { onSelect: openCharacter });

  const search = document.querySelector<HTMLInputElement>("#graph-search")!;
  const count = document.querySelector<HTMLSpanElement>("#graph-count")!;
  search.addEventListener("input", () => {
    const matched = activeGraph!.filter(search.value);
    count.textContent = search.value.trim()
      ? `${matched} of ${nodes.length} nodes match`
      : `${nodes.length} nodes · ${edges.length} links`;
  });
  document.querySelector("#graph-refit")!.addEventListener("click", () => activeGraph!.refit());
}

/// The mounted graph, if the map screen is the one showing. Module-level
/// because navigating away replaces the DOM without telling Cytoscape, and
/// the instance has to be destroyed rather than dropped.
let activeGraph: GraphHandle | null = null;

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
        <label class="mono" for="title">CAMPAIGN TITLE (creates a new campaign)</label>
        <input id="title" placeholder="Thornwick" />
      </div>
      <div class="field">
        <label class="mono" for="vault">VAULT PATH</label>
        <div style="display:flex;gap:8px">
          <input id="vault" placeholder="fixtures/vaults/minimal" style="flex:1" />
          <button class="btn" id="browse" type="button">BROWSE…</button>
        </div>
      </div>
      <div class="field" id="folders-field" hidden>
        <label class="mono">WHAT EACH FOLDER HOLDS — Orison's guess is preselected; change only what's wrong</label>
        <div id="folders"></div>
      </div>
      <button class="btn btn-solid" id="compile">COMPILE</button>
      <p id="import-error" class="error mono"></p>
      <div id="import-report"></div>
    </div>
  `,
  );
  attachNav();
  document.querySelector("#browse")!.addEventListener("click", async () => {
    const picked = await open({ directory: true, title: "Choose your vault folder" }).catch(() => null);
    if (typeof picked === "string") {
      (document.querySelector("#vault") as HTMLInputElement).value = picked;
      await showFolders(picked);
    }
  });
  document.querySelector("#vault")!.addEventListener("change", (e) => {
    const path = (e.target as HTMLInputElement).value.trim();
    if (path) void showFolders(path);
  });
  document.querySelector("#compile")!.addEventListener("click", async () => {
    const title = (document.querySelector("#title") as HTMLInputElement).value.trim();
    const vault = (document.querySelector("#vault") as HTMLInputElement).value.trim();
    // Only the rows the player changed: "auto" leaves ingest's own heuristics
    // (including each note's `type:` frontmatter) in charge.
    const folderTypes: Record<string, string> = {};
    document.querySelectorAll<HTMLSelectElement>("#folders select").forEach((sel) => {
      if (sel.value !== "auto") folderTypes[sel.dataset.folder!] = sel.value;
    });
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
      const out = document.querySelector<HTMLElement>("#import-report")!;
      out.innerHTML = importSummary(report);
      out.querySelector("#play-now")?.addEventListener("click", () =>
        render({ name: "connect", campaignId: created.id, campaignTitle: title }),
      );
    } catch (e) {
      document.querySelector("#import-error")!.textContent = String(e);
    }
  });
}

/// The compile report in words. Every count is either reassurance or a next
/// step; none is left for the player to interpret.
function importSummary(r: IngestReportDto): string {
  const n = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;
  const good: string[] = [
    `Read ${n(r.notesSeen, "note")} and made ${n(r.chunks, "passage")} the story can look up.`,
  ];
  if (r.unaccountedSections.length === 0) good.push("Nothing from your notes was lost.");
  const check: string[] = [];
  if (r.notesWithoutAType > 0)
    check.push(
      `${n(r.notesWithoutAType, "note")} couldn't be told apart (character, place, lore…) and were kept as plain notes. Pick a type for their folders above and compile again if that matters.`,
    );
  if (r.danglingLinkCount > 0)
    check.push(`${n(r.danglingLinkCount, "link")} point at notes that don't exist in the vault.`);
  if (r.genderConflictCount > 0)
    check.push(`${n(r.genderConflictCount, "character")} described with conflicting genders — worth a read.`);
  if (r.unaccountedSections.length > 0)
    check.push(`Text from ${n(r.unaccountedSections.length, "section")} couldn't be placed: ${r.unaccountedSections.join(", ")}.`);
  const list = (items: string[]) => items.map((i) => `<li>${escapeHtml(i)}</li>`).join("");
  return `
    <div class="sheet-block">
      <div class="mono meta">COMPILED</div>
      <ul>${list(good)}</ul>
      ${check.length ? `<div class="mono meta">WORTH A LOOK</div><ul>${list(check)}</ul>` : ""}
      <button class="btn btn-solid" id="play-now">PLAY THIS CAMPAIGN</button>
    </div>`;
}

const FOLDER_KINDS = ["character", "location", "scene", "fauna", "flora", "lore", "item", "note"];

async function showFolders(vault: string) {
  const field = document.querySelector<HTMLElement>("#folders-field")!;
  const list = document.querySelector<HTMLDivElement>("#folders")!;
  const error = document.querySelector("#import-error")!;
  error.textContent = "";
  try {
    const folders = await invoke<VaultFolder[]>("scan_vault_folders", { vault });
    list.innerHTML = folders
      .map(
        (f) => `
      <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;margin:6px 0">
        <span class="mono" id="folder-label-${escapeHtml(f.folder)}">${escapeHtml(f.folder || "(vault root)")} <span class="muted">· ${f.notes} note${f.notes === 1 ? "" : "s"}</span></span>
        <select data-folder="${escapeHtml(f.folder)}" aria-labelledby="folder-label-${escapeHtml(f.folder)}">
          <option value="auto">Auto — ${escapeHtml(f.guess)}</option>
          ${FOLDER_KINDS.map((k) => `<option value="${k}">${k}</option>`).join("")}
        </select>
      </div>`,
      )
      .join("");
    field.hidden = folders.length === 0;
  } catch (e) {
    field.hidden = true;
    error.textContent = String(e);
  }
}

// ---------------------------------------------------------------------------
// Settings (docs/design/Orison.dc.html "settings"). The Lamp/E-ink toggle is
// wired to `document.documentElement.dataset.theme`, which every screen's
// CSS custom properties key off (styles.css `:root[data-theme="eink"]`) — so
// flipping it re-themes the whole shell, not just this screen. Persisted to
// `shell_settings.json` beside the campaign database (#35): `bootstrap()`
// loads it before the first screen renders, and picking a theme here saves
// it back. Verbosity is still a no-op; no command exists to act on it yet.

type ThemeName = "lamplight" | "eink";
interface ShellSettingsDto {
  theme: ThemeName;
}
let currentTheme: ThemeName = "lamplight";

function applyTheme(theme: ThemeName) {
  currentTheme = theme;
  document.documentElement.dataset.theme = theme;
}

function renderSettings() {
  app.innerHTML = shell(
    "settings",
    `
    <div class="pad">
      <h1>The room you read in</h1>
      <div class="sheet-block">
        <div class="mono meta">LIGHT</div>
        <div class="theme-grid" style="margin-top:14px">
          <button class="theme-card${currentTheme === "lamplight" ? " active" : ""}" data-theme-choice="lamplight">
            <div class="theme-swatch lamplight"></div>
            <div class="theme-card-title">Lamplight</div>
            <div class="mono theme-card-caption">warm paper, amber accent</div>
          </button>
          <button class="theme-card${currentTheme === "eink" ? " active" : ""}" data-theme-choice="eink">
            <div class="theme-swatch eink"></div>
            <div class="theme-card-title">E-ink</div>
            <div class="mono theme-card-caption">pure black on white, no motion</div>
          </button>
        </div>
        <p class="muted" style="margin-top:16px">E-ink drops every shadow and animation and holds
        contrast above 10:1.</p>
      </div>
      <div class="sheet-block">
        <div class="mono meta">WHAT LEAVES THIS MACHINE</div>
        <p style="font-size:20px;margin:0">Nothing.</p>
        <p class="muted">Orison reaches one address, and you typed it. No telemetry, no crash reports.</p>
      </div>
      <p class="muted">Verbosity (Instrumented/Narrative/Quiet) is drawn in
      docs/design/Orison.dc.html but not wired to this shell yet — no
      settings command exists to persist it.</p>
    </div>
  `,
  );
  attachNav();
  app.querySelectorAll<HTMLButtonElement>("[data-theme-choice]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const theme = btn.dataset.themeChoice as ThemeName;
      applyTheme(theme);
      renderSettings();
      try {
        await invoke("save_settings", { settings: { theme } });
      } catch (e) {
        console.error("Failed to save shell settings:", e);
      }
    });
  });
}

// Loaded before the first screen renders (#35), so the shell opens in
// whatever theme the player left it in rather than flashing the default.
// A missing or corrupt shell_settings.json is not an error worth stopping
// startup for — `get_settings` already falls back to defaults on the Rust
// side, so this only needs to guard against the IPC call itself failing.
async function bootstrap() {
  try {
    const settings = await invoke<ShellSettingsDto>("get_settings");
    applyTheme(settings.theme);
  } catch (e) {
    console.error("Failed to load shell settings:", e);
  }
  render({ name: "campaigns" });
}

bootstrap();
