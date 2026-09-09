#!/usr/bin/env python3
"""Generate the `large/` fixture vault.

Generated rather than committed file-by-file so that review stays reviewable and
the corpus can be regrown at a different size. The seed is fixed, so the output
is byte-identical on every run and the ground-truth labels below stay valid.

    python3 fixtures/generate_large_vault.py

What it is for: `minimal/` and `messy/` test correctness, this one tests scale.
Retrieval quality and compile time are meaningless on five files. The retrieval
labels deliberately include several rare proper nouns that appear in exactly one
note, because that is the case dense-only embedding search handles worst and it
is the argument for hybrid retrieval in migration_plan.md 3.4.
"""

import json
import random
import shutil
from pathlib import Path

SEED = 20260907
MIN_NOTES = 200  # asserted below; the plan calls for a 200+ note corpus

ROOT = Path(__file__).parent / "vaults" / "large"

# Deliberately invented, low-frequency tokens. A real embedding model has never
# seen these, which is exactly the point.
HOUSES = ["Vantareth", "Oskorby", "Mellichaine", "Drewhollow", "Sarnifex"]
REGIONS = ["the Pale Reach", "Grennalow", "the Ashen Steps", "Kirrow Bight", "Undermarch"]
ROLES = ["archivist", "toll-keeper", "reeve", "salt-factor", "marsh-warden",
         "bell-ringer", "assayer", "ferrymaster", "hedge-doctor", "cartwright"]
TRAITS = ["patient", "abrasive", "superstitious", "meticulous", "credulous",
          "sardonic", "guarded", "effusive", "literal-minded", "conspiratorial"]
GOALS = ["settle an old debt", "leave before the frost", "find a missing sibling",
         "buy back a confiscated boat", "be forgiven", "prove a boundary claim"]

GIVEN_M = ["Halvard", "Corin", "Emeric", "Rulf", "Tobin", "Aldous", "Pell"]
GIVEN_F = ["Ingrith", "Sabeth", "Orla", "Wendeline", "Kestrel", "Aud", "Ferrin"]


def w(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def main() -> None:
    rng = random.Random(SEED)
    if ROOT.exists():
        shutil.rmtree(ROOT)

    characters, locations, lore = [], [], []

    # Locations first, so characters can link to real ones.
    for region in REGIONS:
        for i in range(6):
            name = f"{region.replace('the ', '').title()} {['Landing','Mill','Chapel','Yard','Gate','Barrow'][i]}"
            locations.append((name, region))

    for name, region in locations:
        w(ROOT / "places" / f"{name}.md", f"""---
type: location
name: {name}
tags: [{region.replace('the ', '').lower()}]
---

A {rng.choice(['cramped', 'windblown', 'sunken', 'well-kept', 'half-abandoned'])} place in {region}.
{rng.choice(['The road in is poor.', 'It floods in spring.', 'Nobody stays long.', 'It has its own bell.'])}
""")

    # Characters. Names must be unique or files overwrite each other and the
    # corpus silently shrinks, so collisions get a deterministic epithet.
    used: set[str] = set()
    EPITHETS = ["the Younger", "the Elder", "the Third", "of the Weir",
                "of the Low Road", "the Quiet", "the Lame", "the Widow's Son"]
    for i in range(170):
        female = rng.random() < 0.5
        given = rng.choice(GIVEN_F if female else GIVEN_M)
        house = rng.choice(HOUSES)
        base = f"{given} {house}"
        name = base
        n = 0
        while name in used:
            name = f"{base} {EPITHETS[n % len(EPITHETS)]}" if n < len(EPITHETS) else f"{base} ({n})"
            n += 1
        used.add(name)
        loc = rng.choice(locations)[0]
        characters.append((name, loc))
        w(ROOT / "characters" / f"{name}.md", f"""---
type: character
name: {name}
gender: {'female, she/her' if female else 'male, he/him'}
tags: [{house.lower()}]
---

## Biography

{name} is the {rng.choice(ROLES)} at [[{loc}]]. {rng.choice([
    'Came from away and has never explained why.',
    'Third generation in the same trade.',
    'Took over when the previous holder died suddenly.',
    'Bought the position and regrets it.',
])}

## Personality

{rng.choice(TRAITS).capitalize()} and {rng.choice(TRAITS)}.

## Goals

Wants to {rng.choice(GOALS)}.
""")

    # Lore, including the planted needles the retrieval labels point at.
    for house in HOUSES:
        lore.append(f"House {house}")
        w(ROOT / "lore" / f"House {house}.md", f"""---
type: lore
name: House {house}
---

House {house} holds land across {rng.choice(REGIONS)}. Its claim rests on a
grant nobody has produced in living memory.
""")

    # NEEDLE 1: a unique token appearing in exactly one note.
    w(ROOT / "lore" / "The Quillion Accord.md", """---
type: lore
name: The Quillion Accord
---

The Quillion Accord ended the boundary war between Vantareth and Oskorby. Its
sole surviving copy is held at Pale Reach Chapel. The Accord is the only reason
the two houses share a border rather than a battlefield.
""")

    # NEEDLE 2: thematic, no shared proper noun with its query.
    w(ROOT / "lore" / "On Debt Bondage.md", """---
type: lore
name: On Debt Bondage
---

A person who cannot pay may sell a year of labour. The practice is legal, widely
used, and quietly detested. Most who enter it do not leave in one year.
""")

    notes = len(list(ROOT.rglob("*.md")))
    if notes < MIN_NOTES:
        raise SystemExit(
            f"Generated only {notes} notes, below the {MIN_NOTES} minimum. "
            "Name collisions are silently overwriting files."
        )
    ground_truth = {
        "fixture": {
            "name": "large",
            "description": ("Scale fixture, generated by fixtures/generate_large_vault.py with a "
                            "fixed seed. Tests retrieval quality and compile time, not correctness. "
                            "Regenerate rather than hand-edit."),
            "file_count": notes,
            "generated": True,
            "seed": SEED,
        },
        "performance_budgets": {
            "note": "Not yet measured. Fill from the first live baseline run, then treat regressions as failures.",
            "compile_seconds_p50": None,
            "retrieval_ms_p95": None,
        },
        "retrieval": [
            {
                "query": "what stopped the boundary war",
                "relevant": ["The Quillion Accord"],
                "k": 10,
                "why": ("Needle in a haystack. 'Quillion' appears in exactly one note out of "
                        f"{notes} and nowhere in the query, so this is the paraphrase half of "
                        "the pair with the query below and should favour dense retrieval. Note "
                        "that BM25 also answers it, at rank 1: the target note contains the "
                        "phrase 'boundary war' verbatim, and BM25 needs a distinctive term "
                        "rather than a globally rare one. An earlier version of this field "
                        "claimed BM25 could not, which measurement disproved. See "
                        "docs/eval_baseline.md, 'Phase 3 measurement'."),
            },
            {
                "query": "Quillion",
                "relevant": ["The Quillion Accord"],
                "k": 10,
                "why": ("The same needle by its rare proper noun. BM25 should nail this and dense "
                        "should struggle, since the token is out of vocabulary for the embedding "
                        "model. The two queries together are the clearest single argument in the "
                        "fixture set for hybrid retrieval with rank fusion."),
            },
            {
                "query": "selling a year of your labour to clear what you owe",
                "relevant": ["On Debt Bondage"],
                "k": 10,
                "why": "Pure paraphrase, zero lexical overlap beyond stopwords. Dense-favourable.",
            },
            {
                "query": "who keeps the accord",
                "relevant": ["The Quillion Accord", "Pale Reach Chapel"],
                "k": 10,
                "why": "Two-hop: the Accord names its location, the location has its own note.",
            },
        ],
    }
    w(ROOT / "ground_truth.json", json.dumps(ground_truth, indent=2) + "\n")
    print(f"Generated {notes} notes in {ROOT}")


if __name__ == "__main__":
    main()
