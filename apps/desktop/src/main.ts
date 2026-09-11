// Orison desktop shell — proof-of-wiring scaffold (migration_plan.md §6.1).
//
// This is not the round 2 design canvas ported yet (docs/design/Orison.dc.html
// has all nine screens; see docs/design/README.md). It is the minimum that
// proves the whole stack end to end: list a real campaign from the database,
// connect real models, submit a real turn, and render the real TurnEvent
// stream — so the design can be ported onto a wire that is already known to
// work, rather than against a guess at the command layer's shape.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface CampaignSummary {
  id: string;
  title: string;
  last_played: string;
  playtime_seconds: number;
  active_scene: string;
}

// Mirrors orison_core::turn::TurnEvent. Externally tagged by serde's
// default representation: `{ "StreamDelta": { "text": "..." } }`.
type TurnEvent =
  | { StateChanged: string }
  | { DirectorStateChanged: string }
  | { PlayerMessage: { text: string } }
  | { StreamStarted: { speaker: Speaker; field: string } }
  | { StreamDelta: { text: string } }
  | { StreamEnded: { field: string } }
  | { Message: { speaker: Speaker; text: string } }
  | { SystemMessage: { text: string } }
  | { Failed: { kind: string; detail: string } }
  | { TurnCompleted: null };

type Speaker =
  | "Player"
  | { Character: string }
  | "Narrator"
  | "System";

const app = document.querySelector<HTMLDivElement>("#app")!;

async function main() {
  const campaigns = await invoke<CampaignSummary[]>("list_campaigns").catch(
    (e) => {
      renderError(String(e));
      return [] as CampaignSummary[];
    },
  );
  renderCampaignList(campaigns);
}

function renderError(message: string) {
  app.innerHTML = `<div class="campaign-list"><p class="error mono">${escapeHtml(message)}</p></div>`;
}

function renderCampaignList(campaigns: CampaignSummary[]) {
  app.innerHTML = `
    <div class="rail">ORISON · local · offline · yours</div>
    <div class="campaign-list">
      <h1>Pick up where you left off</h1>
      ${
        campaigns.length === 0
          ? `<p class="mono">No campaigns yet. Create one with the CLI (\`orison new\`) to try this screen.</p>`
          : campaigns
              .map(
                (c) => `
        <div class="campaign-row">
          <div>
            <div style="font-size:26px">${escapeHtml(c.title)}</div>
            <div class="mono" style="font-size:10.5px;color:var(--ink-3);margin-top:6px">
              ${escapeHtml(c.id)} · last played ${escapeHtml(c.last_played)} · ${Math.round(c.playtime_seconds / 60)} min
            </div>
          </div>
          <button data-id="${escapeHtml(c.id)}">RESUME</button>
        </div>`,
              )
              .join("")
      }
    </div>
  `;
  app.querySelectorAll<HTMLButtonElement>("button[data-id]").forEach((btn) => {
    btn.addEventListener("click", () => openCampaign(btn.dataset.id!));
  });
}

async function openCampaign(campaignId: string) {
  app.innerHTML = `
    <div class="rail">ORISON · connecting models</div>
    <div class="campaign-list">
      <p class="mono">Actor model (must already be pulled in Ollama):</p>
      <input id="actor-model" class="mono" style="font-size:14px;padding:8px;width:280px" value="llama3.2:3b" />
      <button id="connect" class="mono" style="margin-left:10px;padding:8px 16px">CONNECT</button>
      <p id="connect-error" class="error mono"></p>
    </div>
  `;
  document.querySelector("#connect")!.addEventListener("click", async () => {
    const actorModel = (document.querySelector("#actor-model") as HTMLInputElement).value;
    try {
      await invoke("connect_models", {
        campaignId,
        args: {
          url: "http://127.0.0.1:11434",
          actorModel,
          directorModel: null,
          twoCalls: false,
          contextLimit: null,
        },
      });
      renderPlay(campaignId);
    } catch (e) {
      document.querySelector("#connect-error")!.textContent = String(e);
    }
  });
}

async function renderPlay(campaignId: string) {
  app.innerHTML = `
    <div class="rail">ORISON · playing ${escapeHtml(campaignId)}</div>
    <div class="campaign-list">
      <div class="sheet">
        <div id="transcript" class="transcript"></div>
        <div class="composer">
          <span class="mono" style="color:var(--lamp)">&rsaquo;</span>
          <input id="draft" placeholder="say something, or type / for commands" />
        </div>
      </div>
    </div>
  `;
  const transcript = document.querySelector<HTMLDivElement>("#transcript")!;
  const draft = document.querySelector<HTMLInputElement>("#draft")!;

  await listen<{ campaignId: string; event: TurnEvent }>("turn-event", (e) => {
    if (e.payload.campaignId !== campaignId) return;
    appendEvent(transcript, e.payload.event);
  });

  draft.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || !draft.value.trim()) return;
    const text = draft.value;
    draft.value = "";
    appendLine(transcript, "YOU", text);
    await invoke("submit_player_input", { campaignId, text }).catch((err) =>
      appendLine(transcript, "ERROR", String(err)),
    );
  });
}

function appendEvent(transcript: HTMLDivElement, event: TurnEvent) {
  if ("StreamDelta" in event) {
    transcript.lastElementChild?.classList.contains("streaming")
      ? (transcript.lastElementChild.textContent += event.StreamDelta.text)
      : appendLine(transcript, "", event.StreamDelta.text, "streaming");
  } else if ("Message" in event) {
    const label = speakerLabel(event.Message.speaker);
    appendLine(transcript, label, event.Message.text);
  } else if ("SystemMessage" in event) {
    appendLine(transcript, "SYSTEM", event.SystemMessage.text);
  } else if ("Failed" in event) {
    appendLine(transcript, "FAILED", `${event.Failed.kind}: ${event.Failed.detail}`, "error");
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

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

main();
