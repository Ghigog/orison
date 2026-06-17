# Orison Technical Design Document (TDD) & Proposal

This document outlines the technical architecture, data structures, and local model orchestration pipeline for **Orison**. It has been updated to prioritize a native Godot architecture, full-screen focus, and JSON-based memory graphs.

---

## 1. Executive Summary & Core Pillars

Orison is a lightweight, cross-platform interactive storytelling and role-playing game engine built in Godot 4.x. It compiles local Markdown folders (Obsidian vaults) into playable visual novel scenes or D&D-style adventures.

```text
+---------------------------------------------------------------+
|                        GODOT FRONT-END                        |
|   - Immersive Full-Screen / Responsive Desktop & Mobile UI    |
|   - Dialogue Display & Choice Selection Nodes                 |
|   - Audio Bridge (Sound FX & TTS Stubs for Future Releases)   |
+-------------------------------+-+-----------------------------+
                                |
                   Internal API | JSON File Reads/Writes
                   & RAG System | (user:// directory)
                                v
+-------------------------------+-+-----------------------------+
|                         LOCAL BACK-END                        |
|  [JSON Memory Graph Engine]   | [Ollama API Engine]           |
|  - World Lore Cache           | - LLM Dialogue & Dungeon Master
|  - Campaign Save Files        | - Structured Tool JSON Outputs
|  - Entity Nodes & Edges       |                               |
+-------------------------------+-------------------------------+
```

* **Pillar 1: Complete Sovereignty:** Local LLM inference (via Ollama or custom local servers) and local JSON file persistence ensure absolute user privacy.
* **Pillar 2: Immersive Atmosphere:** A full-screen, responsive visual presentation with clean layouts, focusing the player’s attention on character art, story text, and choices.
* **Pillar 3: Zero-Dependency Portability:** Relying strictly on native Godot scripting (GDScript) for parsing, memory management, and file storage, ensuring immediate deployment to PC, Mac, Linux, iOS, Android, and Web without compiling platform-specific binary database extensions.

---

## 2. Front-End UI: Responsive Godot Architecture

The UI is designed to scale dynamically across desktop and mobile form factors using Godot's container nodes.

### Viewport Configuration
1. **Adaptive Windowing:** Default to windowed mode with a standard size (e.g. 1280x720) that is resizable. Allow toggling to full screen:
   ```gdscript
   DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN)
   ```
2. **Layout Controls:** Use Godot's `PanelContainer`, `MarginContainer`, and `VBoxContainer` for UI boundaries, ensuring visual novels scale automatically to phone screens (vertical/horizontal layouts) and ultra-wide desktop monitors.
3. **Atmospheric Styling:** Build UI themes using custom `StyleBoxFlat` panels with rounded corners, subtle dropshadows, and custom font weights to create a premium, clean aesthetic.

---

## 3. Data & Memory Layer: Native JSON State Engine

Instead of shipping and compiling SQLite GDExtensions, Orison stores game state, character profiles, quest variables, and relationship graphs inside native Godot JSON files located in the `user://` directory.

### Save State Document Schema
Each active adventure campaign is serialized to a single, structured JSON document (`user://adventures/campaign_id.json`), containing the following blocks:

```json
{
  "adventure_meta": {
    "campaign_id": "dnd_lost_mines",
    "title": "Lost Mine of Phandelver",
    "created_at": "2026-06-15T12:20:00Z",
    "last_played": "2026-06-15T12:20:00Z",
    "active_scene": "town_square"
  },
  "plot_states": {
    "is_drawbridge_down": false,
    "goblin_ambush_triggered": true,
    "town_master_saved": false
  },
  "characters": {
    "char_01_elara": {
      "name": "Elara the Mage",
      "biography": "A weary scholar seeking ancient magic.",
      "affinity": 0.2,
      "inventory": [
        { "item": "staff_of_light", "quantity": 1, "properties": {} },
        { "item": "gold_pieces", "quantity": 50, "properties": {} }
      ],
      "emotions": [
        {
          "timestamp": "2026-06-15T12:18:00Z",
          "emotion": "serenity",
          "intensity": 0.8,
          "target": "player",
          "context": "Player agreed to translate the scroll."
        }
      ]
    }
  },
  "history_logs": [
    { "role": "user", "content": "Let's investigate the ruins.", "timestamp": "2026-06-15T12:17:00Z" },
    { "role": "assistant", "content": "Elara nods, gripping her staff.", "timestamp": "2026-06-15T12:18:00Z" }
  ],
  "knowledge_graph": {
    "nodes": {
      "loc_phandalin": { "label": "Phandalin", "type": "location", "desc": "A rough frontier town built on ruins." },
      "char_elara": { "label": "Elara", "type": "npc", "desc": "Wields fire magic, trusts the player." }
    },
    "edges": [
      { "from": "char_elara", "to": "loc_phandalin", "relation": "visiting", "weight": 1.0 }
    ]
  }
}
```

### Advantages of the JSON/Resource Model:
1. **Atomic Saves:** Writing the state is a simple JSON string dump:
   ```gdscript
   var file = FileAccess.open("user://adventures/campaign.json", FileAccess.WRITE)
   file.store_string(JSON.stringify(save_data))
   ```
2. **Zero Setup Friction:** Native Godot engines read and write to `user://` on iOS, Android, macOS, Windows, Linux, and HTML5 Web exports without permission blocks or library compile crashes.
3. **Easy Versioning:** Save files are text documents, making debugging and user-shared save states trivial.

---

## 4. Local AI Orchestration & Tool calling

Orison remains backend-agnostic, targeting local HTTP APIs (such as Ollama on port `11434` or llama.cpp on port `8080`).

### Context Synthesis (Sliding Prompt Window)
Local 8B parameter models are constrained by VRAM and context lengths. To maximize accuracy:
1. **Immediate Context:** Inject only the last 8-10 turns of dialogue from `history_logs`.
2. **Dynamic State Injection:** Inject the serialized dynamic states (current inventory, plot flags, nearby NPCs and their current affinity score).
3. **Read-Only Lore Retrieval:** Search the compiled Obsidian vault entries matching keywords in the player's last prompt, retrieving relevant world details and injecting them as static background context.
4. **Graph Traversal:** Identify entities mentioned in the prompt, find connected nodes in the JSON `knowledge_graph`, and add the descriptions of connected edges to the context.

---

## 5. Technical Risks & Mitigations

### 1. Model Parsing Failures
* **Risk:** Small local models (3B to 8B) fail to return valid JSON formats.
* **Mitigation:** Godot will implement a strict Regex parser that identifies the outermost `{ ... }` curly braces and strips conversational prefixes (e.g. "Sure, here is your update:") or trailing commas before attempting to deserialize JSON responses.

### 2. The VRAM Wall (Hardware Constraints)
* **Risk:** Running LLM inference, local text-to-speech (TTS), and image generation at the same time locks the consumer GPU, causing out-of-memory (OOM) failures.
* **Mitigation:** 
  * **Phase 1 Release Scope:** Sound FX and Stable Diffusion image generation are deferred to future updates. The initial release uses pre-compiled local media packs (illustrations, sound loops) linked in the Markdown vaults.
  * **Sequenced Execution Pipeline:** Future media updates will enforce a sequenced queue: Text streaming completes first, releasing the GPU before TTS or visual models are loaded.
