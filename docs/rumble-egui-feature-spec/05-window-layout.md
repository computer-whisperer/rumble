# §5 — Window Layout

The top-level shell is a stack of egui panels around a central area.

| Region | Type | Where | Purpose |
|---|---|---|---|
| Top menu bar | `TopBottomPanel::top("top_panel")` | `app.rs:3632-3727` | Server/Settings/File-Transfer menus + right-aligned status pill. |
| Toolbar | `TopBottomPanel::top("toolbar_panel")` | `app.rs:3730-3879` | Mute/Deafen/Voice mode/Elevate/Settings gear. |
| Left side | `SidePanel::left("left_panel")`, default 320px, resizable | `app.rs:3882-4056` | Chat panel (header + history + input). |
| Center | `CentralPanel::default` | `app.rs:4059-4654` | Room / user tree. |

There is **no** bottom status bar, **no** docked right panel, **no** tray
icon, **no** always-on-top, **no** minimize-to-tray.

A drag-hover overlay is drawn as an `egui::Area` at `Order::Foreground`
when files are being dragged over the window (see §10.5).
