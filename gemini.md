# Gemini Agent Guide

This document assists AI agents (like Antigravity) in navigating the Orison codebase, finding relevant features, and keeping development modular.

## Agent Guidelines

1. **Keep Code Modular**: Keep scripts focused on specific systems (e.g., Parser, State Manager, UI). Avoid monolithic files.
2. **Document Feature Locations**: If a feature is implemented inside a shared script rather than having its own dedicated script, add or update its exact line range in the **Feature Map** below.
3. **Keep this File Updated**: Every time you implement, refactor, or delete a feature, update the Feature Map and/or files list below.
4. **Follow the Ticket System**: Check [docs/in_progress.md](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/in_progress.md) for current tasks. Move tickets between tracking files (`backlog.md` -> `in_progress.md` -> `done.md`) as status changes.
5. **Adhere to the Design System**: When developing or updating user interfaces, themes, or layouts, strictly follow the specifications outlined in the [Design Philosophy](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/design_philosophy.md) system.

---

## Feature Map

This table maps specific functional features of Orison to their implementation scripts and line numbers.

| Feature ID | Feature Name | Target Platform | Script / File Path | Line Range / Class | Status | Description |
|---|---|---|---|---|---|---|
| *SYS001* | *Example Feature* | *All* | *res://example.gd* | *L10-L25* | *Placeholder* | *An example description of a feature* |
| SYS002 | System Prompts | All | res://src/core/SystemPrompts.gd | class_name SystemPrompts | Active | Dynamic system prompt generation for the World Builder (DM) and Character Agents. |
| SYS003 | Onboarding Flow | All | res://src/ui/OnboardingFlow.gd | class_name OnboardingFlow | Active | Modal welcome overlay guiding vault import, offering a sample adventure, loading saved campaigns, and warning on duplicate name overwrite. |
| SYS004 | Full-Screen Dialogue UI | All | res://src/ui/MainViewport.gd | class_name MainViewport | Active | Layered, premium overlay layout with collapsible sidebar, dynamic nameplate colors, and rapport progress indicators. |
| SYS005 | Sidebar Emotion Display | All | res://src/ui/CharacterListItem.gd | class_name CharacterListItem | Active | Displays character feelings, intensities, and reasons dynamically in the sidebar. |
| SYS006 | Writing Style Extraction | All | res://src/core/VaultCompiler.gd | class_name VaultCompiler | Active | Extracts campaign-wide and character-specific writing style/dialogue snippets from Markdown files. |
| SYS007 | Character Image Extraction | All | res://src/core/VaultCompiler.gd | class_name VaultCompiler | Active | Scans vault directories for character-related PNG/JPEG images, copies them to user assets, and resolves them dynamically. |
| SYS008 | Interactive Onboarding Review | All | res://src/ui/OnboardingFlow.gd | class_name OnboardingFlow | Active | Scans campaign vaults to display entities count and uses the local LLM to generate 3 starting hooks for the user to select. |
| SYS009 | Location Connection Extraction | All | res://src/core/VaultCompiler.gd | class_name VaultCompiler | Active | Extracts character-to-location links and tags during compilation to create associated_with edges in the knowledge graph. |
| SYS010 | Location-Filtered Sidebar List | All | res://src/ui/MainViewport.gd | class_name MainViewport | Active | Filters the nearby characters sidebar list to show only characters relevant to the active location. |
| SYS011 | Character Avatar Display | All | res://src/ui/CharacterListItem.gd | class_name CharacterListItem | Active | Displays character profile pictures/avatars in the sidebar character list items. |
| SYS012 | Narrative DM Memory Integration | All | res://src/ui/MainViewport.gd | class_name MainViewport | Active | Sequentially runs the Narrative DM (World Builder) and Character Agent models on every player input, maintaining short, medium, and long-term campaign memories in CampaignState, and updating the UI sidebar display. |
| SYS013 | Theme Customization System | All | res://src/autoload/ThemeManager.gd | class_name ThemeManager | Active | Manages color scheme presets and custom user-saved themes. Applies colors and font sizes directly to 7 Theme Type Variations (`LabelTitle`, `LabelMuted`, `LabelSubtle`, `LabelSmall`, `LabelAccent`, `RichTextSmall`, `ButtonAccent`) defined in `orison_ui.tres` via `apply_active_theme()`; no scene-tree traversal needed. |
| SYS014 | Universal Font Size Scaling | All | res://src/autoload/ThemeManager.gd | class_name ThemeManager | Active | Scales font sizes via `font_size_modifier` written directly onto each Theme Type Variation in `apply_active_theme()`. The deprecated `apply_theme_to_hierarchy()` traversal is no longer called. |
| SYS015 | Setup Mind Map | All | res://src/ui/CampaignGraphView.gd | class_name CampaignGraphView | Active | Interactive node graph using GraphEdit with a deterministic location-anchored 2D grid/table layout (Locations, Lore, Scenes, Characters), loaded in-game via MindMapModal for real-time visualization and editing. |
| SYS016 | Tweened Menu Transitions | All | res://src/ui/OnboardingFlow.gd | class_name OnboardingFlow | Active | Performs smooth card reshaping and fade transitions between onboarding menus and settings modals. |
| SYS017 | Director/Actor Split Game Loop | All | res://src/ui/MainViewport.gd | class_name MainViewport | Active | Splits the game loop into foreground NPC streaming response and background asynchronous DM generation. |
| SYS018 | Ollama HTTP Streaming Parser | All | res://src/autoload/LLMClient.gd | class_name LLMClient | Active | Asynchronously parses chunked HTTP responses from Ollama to enable token-by-token text streaming. |
| SYS019 | NPC Environmental/Narrative Emotion Reflection | All | res://src/ui/MainViewport.gd | L975-L1018 | Active | Deduces starting base emotions from character biographies and triggers the active NPC to reflect on environmental events/narration beats. |
| SYS020 | Visual Novel Input Syntax Parsing | All | res://src/core/PlayerInputParser.gd | class_name PlayerInputParser | Active | Parses player inputs into dialogue and action/context segments based on visual novel formatting cues (quotes, asterisks, pronouns). |
| SYS021 | Player Character Creation | All | res://src/ui/OnboardingFlow.gd | class_name OnboardingFlow | Active | Player character creation with Name, Avatar picker, physical description summary generator, personality, and backstory fields. |


---

## Codebase Map

### Core Systems

- **Parser Engine**: Parses Obsidian markdown archives and player input formatting (`res://src/core/MarkdownParser.gd`, `res://src/core/VaultCompiler.gd`, `res://src/core/VaultScanner.gd`, `res://src/core/PlayerInputParser.gd`).
- **Story State Manager**: Coordinates game state, inventory updates, and saving/loading (`res://src/autoload/CampaignState.gd`, `res://src/core/SaveManager.gd`).
- **UI & Layout**: Controls viewport rendering, dialogue text scrolls, settings configurations, theme/font customization, and responsive visual layout maps (`res://scenes/ui/MainViewport.tscn`, `res://src/ui/MainViewport.gd`, `res://src/autoload/ThemeManager.gd`, `res://src/ui/SettingsModal.gd`, `res://src/ui/CharacterVisuals.gd`, `res://src/ui/MindMapModal.gd`).
- **Tabletop/DND Ruleset**: Handles dice rolls, character stats, and rules checks (future implementation).
- **Emotion Engine**: Processes character emotional states and rapport changes (`res://src/core/EmotionEngine.gd`, `res://src/core/EmotionPromptBuilder.gd`).
- **AI Prompting & Clients**: Manages local LLM prompting, custom system instructions, and response parsing (`res://src/core/PromptBuilder.gd`, `res://src/core/SystemPrompts.gd`, `res://src/autoload/LLMClient.gd`, `res://src/core/JsonRepair.gd`).
- **Autoload Messaging**: Manages decoupled signals across modules (`res://src/autoload/EventBus.gd`).
