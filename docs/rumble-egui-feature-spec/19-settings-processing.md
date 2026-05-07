# §19 — Settings — Processing (audio pipeline editor)

`render_settings_processing`, `app.rs:1679-1790`.

- **TX Pipeline** — iterates `pending_tx_pipeline.processors` in order.
  For each processor row:
  - Enable/disable checkbox labelled with `display_name` from
    `ProcessorRegistry`; hover tooltip = description.
  - When enabled, indented sub-section with a dynamic settings form
    generated from the processor's JSON schema (via
    `render_schema_field` at `app.rs:3244-3327`):
    - `number` → `egui::Slider` with min/max from schema.
    - `integer` → integer slider.
    - `boolean` → checkbox.
    - else → text input.
  - Each field shows `title` from schema and `description` as hover.

Built-in processors (registered by `register_builtin_processors` in
`crates/rumble-client/src/processors/mod.rs:34-38`):

- `builtin.gain` — volume adjustment (default ON).
- `builtin.denoise` — RNNoise (default ON).
- `builtin.vad` — voice activity detection (default OFF).

Order is fixed by `DEFAULT_TX_PIPELINE` (Gain → Denoise → VAD).
**There is no drag-reorder UI** and **no preset system** — the only
"preset" is `build_default_tx_pipeline()`.

The input level meter (with VAD threshold line) is duplicated below the
pipeline editor for convenient threshold tuning.

There is no RX pipeline editor, although `state.audio.rx_pipeline_defaults`
exists in state.

Apply sends `Command::UpdateTxPipeline { config: PipelineConfig }`.
