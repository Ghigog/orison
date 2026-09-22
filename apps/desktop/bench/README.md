# Transcript benchmarks

Not part of CI, and not a dependency of the app. These exist so the
performance claims in `src/styles.css` and `docs/handoff_phase6.md` can be
re-run rather than believed.

```sh
npm install --no-save playwright
npx playwright install chromium
node bench/transcript-streaming.mjs
```

`ORISON_BENCH_CHROMIUM` points at an existing Chromium binary if you already
have one and would rather not download another.

## `transcript-streaming.mjs`

Answers the question `migration_plan.md` §6.4 assumed the answer to: does the
transcript need a JS list virtualiser?

It measures the cost that actually scales — the synchronous layout forced by
`scrollTop = scrollHeight` on every streamed delta — across three arms: the
DOM as written, the same DOM under `content-visibility`, and an idealised
windowed list that holds only the visible rows. The third is a ceiling no
library reaches, not a library's real number, which is the point: if the
ceiling loses, there is nothing to buy.

It lost. See the table in `src/styles.css` above the containment rule.

## `graph-preview/`

A page that mounts `src/graph.ts` with fixture nodes and edges, so the
knowledge graph can be looked at without a compiled vault, a running Ollama,
or the Tauri shell around it.

```sh
npx vite bench/graph-preview --port 5199
# then open http://localhost:5199/?theme=lamplight
#      and  http://localhost:5199/?theme=eink
```

It is here because a `<canvas>` cannot be asserted on the way a DOM tree can,
and the one bug that mattered most in building the graph was only visible by
looking: Cytoscape parses colours with its own parser, which does not
understand `oklch()`, so every design token silently resolved to black. The
graph rendered, laid out, filtered and reported correct node counts the whole
time it was doing this.
