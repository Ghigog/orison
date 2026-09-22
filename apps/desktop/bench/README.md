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
