# Orison Emotion & Rapport System

This document defines the **Tag-Based Memory Emotion System** for NPCs and Story/Dungeon Master (DM) agents in **Orison**. It shapes how characters and narrator agents communicate, maintaining emotional state and relationship logs in their memory graph.

---

## 1. Core Philosophy

NPC emotional states should be driven by **narrative events and dialogue content**, not system performance (RAG retrieval or database status). 

* **Emotions as Memory Events:** When a character responds to the player, their emotional reaction is tagged and recorded in memory (e.g., `Character A felt Anger about player's decision to spare the goblin`).
* **LLM-Driven Emotional State:** The local LLM determines its own emotional tone, tagging its output with appropriate feelings, intensities, and targets.
* **Persistent Rapport (Affinity):** Relationship values range from `-1.0` (Hostile) to `+1.0` (Devoted), affecting dialogue prompt injections and branching story options.

---

## 2. Emotional Tags & Intensities

Instead of hard-coded dimensions, characters use a standard set of emotional tags, rated by intensity (`0.0` to `1.0`) and directed at a target (e.g., `player`, another NPC, or a world event).

### 2.1 Standard Emotion Palette
Authors can define custom emotions, but the default palette includes:

| Emotion | Default Tone Guidance |
|---|---|
| **Serenity** | Calm, warm, clear, confident. Balanced responses. |
| **Joy** | Upbeat, helpful, cooperative, and enthusiastic. |
| **Sadness** | Gentle, quiet, melancholic, or distant. |
| **Anger** | Clipped, blunt, impatient, or tense. |
| **Fear** | Careful, tentative, defensive, or guarded. |
| **Trust** | Open, vulnerable, supportive. |
| **Disgust** | Cold, dismissive, revolted. |
| **Surprise** | Expressive, unsettled, highly reactive. |

### 2.2 Memory Record Format
Every emotional event is saved as a memory node in the character's record:
```json
{
  "type": "emotion_event",
  "timestamp": "2026-06-15T12:20:00Z",
  "emotion": "anger",
  "intensity": 0.8,
  "target": "player",
  "context": "The player insulted my ancestors."
}
```

---

## 3. The Rapport Meter (Affinity)

The Rapport Meter represents the long-term relationship health between the player and an NPC.

**Range:** `-1.0` (Nemesis) to `+1.0` (Best Friend).

| Range | Level | Dialogue / Behavior Profile |
|---|---|---|
| -1.0 to -0.6 | **Nemesis** | Openly adversarial or hostile. Tries to hinder player progress. |
| -0.59 to -0.2 | **Enemy** | Guarded, cold, and dismissive. Reluctant to share information. |
| -0.19 to +0.19 | **Acquaintance** | Neutral and formal. Transactional interactions. |
| +0.2 to +0.59 | **Friend** | Warm, familiar, cooperative, and helpful. |
| +0.6 to +1.0 | **Best Friend** | Deeply loyal. Offers hidden paths, discounts, and protection. |

---

## 4. Prompt Synthesis & Injection

At the start of a dialogue turn, the active character's emotional memory is injected into their system context:

### 4.1 Injection Block Format
```text
You are {Character_Name}.
Current Emotional Profile:
- Active Feeling: {active_emotion} (Intensity: {intensity}) towards {target}
- Reason: {emotional_context}
- Relationship with Player: {relationship_level} (Affinity Score: {affinity_score})

Rules:
- Let these feelings shape your tone, choices, and reactions naturally.
- Do not state your raw affinity score or emotion JSON parameters directly unless asked.
- Reflect your history in your dialogue. If the player previously angered you, remain cautious or cold.
```

---

## 5. System Architecture Flow

When processing a dialogue turn:

```
[Player Input]
      │
      ▼
[Context Synthesizer] ──► Reads active emotion and history from JSON memory
      │
      ▼
[LLM Request] ──► Injects dialogue context and character emotional profile
      │
      ▼
[LLM Response] ──► Returns text response along with emotional updates
      │
      ├──► [Dialogue Text] ──► Rendered to UI
      │
      └──► [Emotional Update Tag] ──► Saved to Character Memory & updates Rapport
```

### 5.1 JSON Response Tagging Structure
The LLM is prompted to return its response in a structured container containing both the dialogue text and the emotional updates:

```json
{
  "response": "How dare you speak to me that way! Leave my tavern at once.",
  "emotional_update": {
    "emotion": "anger",
    "intensity": 0.9,
    "target": "player",
    "reason": "Player threatened my patron",
    "rapport_delta": -0.15
  }
}
```
If the local model struggles with strict JSON output, the JSON is extracted/repaired by the engine before updating the character's memory state.
