//! The `PromptBuilder` / `SystemPrompts` boundary, enforced rather than
//! documented (§4.5).
//!
//! [orison_audit.md §14]: "Both PromptBuilder.gd and SystemPrompts.gd
//! participate in prompt construction with overlapping responsibilities. It's
//! unclear where to make changes for a given prompt modification."
//!
//! Both Godot files open with a header stating the rule, and both break it —
//! `SystemPrompts` builds conditional blocks out of a `Dictionary`,
//! `PromptBuilder` writes eleven blocks of prose inline. A comment that the
//! code disagrees with is worse than no comment, so this is a test instead.
//!
//! Same shape as `tests/knowledge_graph.rs`, which fails the build if any
//! module outside `knowledge/` grows a map keyed by `EntityId`: an
//! architectural rule is worth stating only if something checks it.
//!
//! [orison_audit.md §14]: ../../docs/orison_audit.md

use std::path::Path;

fn source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The file with its comments removed.
///
/// The rules below are about what the code does, and a doc comment that
/// *describes* the rule ("this may not import `crate::state`") would otherwise
/// trip it — which would push the explanation out of the file it explains.
fn code(relative: &str) -> String {
    source(relative)
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rule: static prompt text may not read campaign state.
#[test]
fn templates_cannot_reach_campaign_state() {
    let text = code("src/prompt/templates.rs");
    for forbidden in [
        "crate::state",
        "crate::knowledge",
        "crate::retrieval",
        "crate::turn",
        "crate::memory",
        "crate::emotion",
        "CampaignStore",
        "KnowledgeGraph",
    ] {
        assert!(
            !text.contains(forbidden),
            "prompt::templates must contain words only, but references `{forbidden}`. \
             Anything that reads state belongs in prompt::assembly."
        );
    }
}

/// The converse: assembly may read state, but must not be where the words
/// live. Checked by a proxy that is hard to satisfy accidentally — the
/// instruction prose all lives behind `templates::`, so assembly should not
/// contain the phrases that make up a system prompt.
#[test]
fn assembly_does_not_grow_its_own_system_prompt() {
    let text = code("src/prompt/assembly.rs");
    for forbidden in ["CORE RULES", "You are the", "Do not agree with the player"] {
        assert!(
            !text.contains(forbidden),
            "prompt::assembly contains instruction prose (`{forbidden}`). \
             Every word the model is told belongs in prompt::templates."
        );
    }
}

/// And the turn engine assembles nothing itself: it gathers state and hands
/// it over. This is the rule that actually rotted in the Godot build, where
/// `GameLoopController` grew its own prompt fragments.
#[test]
fn the_turn_engine_writes_no_prompt_text() {
    let text = code("src/turn/engine.rs");
    for forbidden in ["CORE RULES", "=== NARRATIVE", "<player_message>"] {
        assert!(
            !text.contains(forbidden),
            "turn::engine contains prompt text (`{forbidden}`). It belongs in prompt::."
        );
    }
}

/// The one piece of formatting that is a security property must have exactly
/// one implementation, so that a line sent this turn and the same line
/// replayed as history next turn cannot drift apart.
#[test]
fn the_player_message_wrapper_has_one_implementation() {
    let mut found = Vec::new();
    for file in [
        "src/prompt/assembly.rs",
        "src/prompt/templates.rs",
        "src/prompt/ordering.rs",
        "src/turn/engine.rs",
    ] {
        let text = code(file);
        // The opening delimiter written as a literal into a format string.
        if text.contains("\"<player_message>\\n") {
            found.push(file);
        }
    }
    assert_eq!(
        found,
        vec!["src/prompt/assembly.rs"],
        "the <player_message> wrapper must be written in exactly one place"
    );
}
