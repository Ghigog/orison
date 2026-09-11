# Orison Design Philosophy & Language

This document covers the **currently shipping Godot build**: the runtime
theming [ThemeManager](src/autoload/ThemeManager.gd) actually implements, and
the accessibility bar it has to clear. The Godot build is in feature freeze
(bug fixes only — see [AGENTS.md](AGENTS.md)), so this is maintenance
reference, not a target to build toward.

It used to also describe a "dark-mode-first, glassmorphic" visual identity —
a spacing scale, border radii, shadow system, per-emotion glow colors, and a
motion-tween choreography for dialogue, buttons and dice rolls. None of that
was ever built; [orison_audit.md §22](docs/orison_audit.md) confirms
`ThemeManager` only ever implemented colors and font sizes, and it defaults to
a **light** theme, not dark. That material is removed rather than corrected.

The forward-looking visual direction — for the Phase 6 Tauri shell — is being
drawn fresh in [docs/design/](docs/design/) and does not inherit the palette
below.

---

## 1. Typography

Two real, implemented font families, set in
[resources/themes/orison_ui.tres](resources/themes/orison_ui.tres):

- **Narrative serif** (dialogue, story prose): Lora, falling back to
  Merriweather, then a generic serif.
- **System sans** (menus, stats, buttons, labels): Outfit, falling back to
  Inter, then a generic sans-serif.

| Element | Family | Size | Weight | Tracking |
| :--- | :--- | :--- | :--- | :--- |
| Chapter title | Sans | 24px | Semi-Bold | +0.05em |
| Speaker nameplate | Sans | 18px | Bold, caps | +0.1em |
| Dialogue text | Serif | 18px | Regular | normal |
| Choice button | Sans | 14px | Medium | +0.05em |
| Sidebar widget title | Sans | 13px | Semi-Bold, caps | +0.08em |
| Micro labels | Sans | 11px | Regular | +0.02em |

Base body size is 14px, adjustable by a single global `font_size_modifier`
applied across the scene tree (`ThemeManager.apply_theme_to_hierarchy`) —
there is no independent per-element scale at runtime, only this one offset.

---

## 2. Color tokens

`ThemeManager` exposes five semantic tokens, switchable at runtime and
persisted to `user://theme_config.json`:

| Token | Usage |
| :--- | :--- |
| `color_bg` | Window/canvas background |
| `color_surface` | Panels, cards, list rows |
| `color_border` | Outlines, dividers, progress-bar backdrops |
| `color_text` | Body and label text |
| `color_accent` | Focus rings, active selection, highlights |

Plus two fixed constants used everywhere rapport/outcome needs a color:
`color_success` (`#10B981`) and `color_danger` (`#E11D48`).

Four presets ship today, defined in `ThemeManager.PRESETS`. **Dawn is the
default** — the build has always opened in a light theme, not the dark one
this document used to lead with:

| Preset | bg | surface | border | text | accent |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Dawn** (default) | `#FAF6F2` | `#FCFAF7` | `#FCDAC7` | `#2C2521` | `#FF5E3A` |
| Ethereal Codex | `#0B0D11` | `#161A22` | `#FFFFFF` | `#F3F4F6` | `#8B5CF6` |
| Daybreak Meadow | `#F4FBF7` | `#F9FDFB` | `#A7F3D0` | `#14291E` | `#10B981` |
| Solstice Obsidian | `#08080C` | `#14141A` | `#808090` | `#E4E4E7` | `#E11D48` |

Players can also save custom five-token combinations as named themes through
the in-app Visual Settings menu.

---

## 3. Accessibility and high-contrast (e-ink) support

[orison_audit.md](docs/orison_audit.md) flags this build as having no
keyboard navigation, no screen-reader support, and **no high-contrast
validation** — none of the four presets above has been checked against a
contrast floor. That's the actual gap; closing it matters more than any
color name.

Requirements going forward, for both this build's bug fixes and Phase 6:

- Body text against its background must clear **4.5:1** contrast (WCAG AA);
  large text and UI chrome, **3:1**.
- Every preset — built-in or player-saved — must be checkable against that
  floor, not just eyeballed.
- At least one preset must work with all motion, blur and glow removed:
  no information may be conveyed by animation or glow alone. The round 2
  Phase 6 canvas's E-ink theme is the model to match — pure black-on-white,
  no shadow or gradient, contrast held above 10:1, and its streaming caret
  replaced by a word-by-word reveal because a blinking cursor forces a
  full-page refresh on real e-ink hardware.

---

## 4. Implementation rules (Godot)

1. **No ad-hoc colors.** Set colors and fonts through
   `res://resources/themes/orison_ui.tres` and `ThemeManager`, never as
   literal values in a node inspector or script.
2. **Save state before visual feedback.** A rapport/outcome notification
   fires only after [SaveManager](src/core/SaveManager.gd) confirms the
   write, not before.
3. **Anchor, don't hardcode.** Use anchor presets so panels survive a window
   resize; no fixed pixel offsets for layout.

---

## 5. Where this is going

The Phase 6 Tauri shell's visual direction — a light "paper on a desk"
surface, not this document's tokens — lives in
[docs/design/](docs/design/README.md). Once that direction is settled, this
file's color and typography sections should be replaced with whatever the
new shell actually implements, the same way this rewrite replaced the
unbuilt "Ethereal Codex" identity with what `ThemeManager` really does.
