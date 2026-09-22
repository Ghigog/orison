// Does the Orison transcript need a list virtualiser (migration_plan.md §6.4)?
//
// The cost that matters is not first paint, it is streaming: every
// StreamDelta appends text and then does
//   transcript.scrollTop = transcript.scrollHeight
// which reads a geometry property and so forces a synchronous layout of the
// whole scroll container. At 20-40 tok/s over a 15-second turn that is
// hundreds of full-transcript layouts, and the cost grows with how much
// transcript there is. So: hold N committed rows, stream 200 deltas into a
// new row, and measure the wall time.
//
// Three arms: the DOM as it is today, the same DOM with CSS containment on
// each row, and an upper bound for any windowing library (only the visible
// rows exist in the DOM at all).

import process from "node:process";
import { chromium } from "playwright";

const ROW_COUNTS = [40, 200, 1000, 5000];
const DELTAS = 200;
const VISIBLE_ROWS = 12; // what a 50vh transcript shows at once

const page_html = `<!doctype html><meta charset="utf-8"><style>
  body { font-family: Georgia, serif; font-size: 17px; line-height: 1.6; margin: 0; }
  .transcript { padding: 20px 30px 8px; max-height: 50vh; overflow: auto; width: 720px; }
  .transcript p { margin: 0 0 14px; }
  .transcript p.speech strong { font-weight: 600; }
  .contained p { content-visibility: auto; contain-intrinsic-size: auto 68px; }
</style><div id="host"></div>`;

const bench = ({ rows, deltas, visibleRows, mode }) => {
  const host = document.querySelector("#host");
  host.innerHTML = "";
  const t = document.createElement("div");
  t.className = mode === "contained" ? "transcript contained" : "transcript";
  host.append(t);

  const LINE =
    "The mill road runs east under a sky the colour of wet slate, and the " +
    "water wheel has not turned since the spring. Bram Holt watches it anyway.";

  // A windowed list only ever holds the visible rows; everything above is a
  // single spacer of the right height. This is the ceiling any virtualiser
  // could reach, not a library's real number.
  const held = mode === "windowed" ? Math.min(rows, visibleRows) : rows;
  if (mode === "windowed" && rows > visibleRows) {
    const spacer = document.createElement("div");
    spacer.style.height = `${(rows - visibleRows) * 68}px`;
    t.append(spacer);
  }
  for (let i = 0; i < held; i++) {
    const p = document.createElement("p");
    p.className = i % 2 ? "speech" : "";
    p.textContent = `${i}. ${LINE}`;
    t.append(p);
  }

  // Force the initial layout so it is not counted in the streaming measure.
  void t.scrollHeight;

  const streaming = document.createElement("p");
  streaming.className = "streaming";
  t.append(streaming);

  const start = performance.now();
  for (let d = 0; d < deltas; d++) {
    streaming.append("token ");
    t.scrollTop = t.scrollHeight; // the forced synchronous layout
  }
  const elapsed = performance.now() - start;
  return { elapsed, perDelta: elapsed / deltas, domNodes: t.querySelectorAll("p").length };
};

const browser = await chromium.launch({ channel: process.env.ORISON_BENCH_CHANNEL || undefined,
  executablePath: process.env.ORISON_BENCH_CHROMIUM || undefined });
const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
await page.setContent(page_html);

const results = [];
for (const rows of ROW_COUNTS) {
  for (const mode of ["plain", "contained", "windowed"]) {
    // Three runs, take the median: a single run on a shared CI-class box is
    // noise. (Phase 5.6's lesson: check the instrument before the verdict.)
    const runs = [];
    for (let i = 0; i < 3; i++) {
      runs.push(await page.evaluate(bench, { rows, deltas: DELTAS, visibleRows: VISIBLE_ROWS, mode }));
    }
    runs.sort((a, b) => a.perDelta - b.perDelta);
    results.push({ rows, mode, ...runs[1] });
  }
}

await browser.close();

console.log(`\n${DELTAS} streamed deltas, median of 3 runs, Chromium headless\n`);
console.log("rows   mode        ms/delta   total ms   <p> in DOM");
for (const r of results) {
  console.log(
    String(r.rows).padEnd(6) +
      r.mode.padEnd(12) +
      r.perDelta.toFixed(3).padStart(8) +
      r.elapsed.toFixed(0).padStart(11) +
      String(r.domNodes).padStart(12),
  );
}
