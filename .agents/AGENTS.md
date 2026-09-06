# Project UI Rules

1. **Prefer Scene Files (.tscn)**: In Godot, always prefer creating UI elements via `.tscn` files and instantiating them dynamically over injecting them programmatically through GDScript code.
2. **Responsive Layouts**: Design all UI to be fully responsive as the window resizes. Use Godot containers (`MarginContainer`, `HBoxContainer`, `VBoxContainer`, `PanelContainer`, `ScrollContainer`) and layout anchors instead of hardcoded coordinates or manual offsets.
3. **No Layout Bleed**: Restrain all control elements within container bounds to prevent them from "bleeding" out or overlapping other panels. Ensure `clip_contents` or appropriate minimum sizes are set on parent controls where appropriate.
