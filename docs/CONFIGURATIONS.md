# Configurations and base features

A design note for two additions to the document model: configurations (table-driven variants
of one design, Fusion's Configurations) and base features (direct, non-parametric edits, Fusion's
"Do not capture design history" mode and Base Feature).

## Configurations

### Data

A design keeps a table of named configurations in `Design::configurations`
(`crates/model/src/config.rs`):

```json
"configurations": {
  "active": "Long",
  "list": [
    {"name": "Short", "overrides": [{"type": "parameter", "name": "length", "expr": "40 mm"}]},
    {"name": "Long", "overrides": [
      {"type": "parameter", "name": "length", "expr": "120 mm"},
      {"type": "suppress", "feature": 7, "suppressed": true},
      {"type": "appearance", "body": 3000, "color": [200, 30, 30]},
      {"type": "joint", "joint": 12, "motion": "rotation", "value": "45 deg"}
    ]}
  ]
}
```

- A configuration is a **sparse row**: it lists only what it changes. Anything it doesn't
  override takes the design's own value, so a design without configurations, or with none
  active, is exactly what it was before.
- The four override kinds are the table's column kinds: a **parameter** expression, a feature's
  **suppression**, a body's **appearance** (colour, or `null` for none) and a **joint** motion
  value (rotation, slide, slide2 or pitch, as an expression).
- The table's columns are the distinct override targets across all rows (`Column`); a cell is
  the row's override for that column or empty (the default).
- `active: null` (or no `configurations` key at all) means "no configuration": the design as
  modelled. Every field has a serde default and is skipped when empty, so existing `.clf` files
  load and save unchanged.

### Regeneration

Overrides never touch the stored design. `config::effective` builds a copy with the active
row applied (parameters replaced, suppression flags set, joint motions replaced) and with no
configuration active; `Regenerator::regenerate` regenerates that copy and writes the
regeneration's own results (solved sketch positions, refreshed face and edge references) back
into the real features, except the joint fields the configuration overrode. Because the copy's
parameters and features feed the regeneration cache key like any edit, switching
configurations recomputes from the first feature that differs, and switching back is a
normal (cached up to that point) rebuild.

Appearances are display data, not geometry: `Design::body_color` returns the active row's
colour for a body, falling back to the body's own setting; the viewport and `info` use it.

Overrides naming a parameter, feature, body or joint that no longer exists are ignored (the
table shows them so they can be cleared). Renaming a parameter renames it in every row and
inside override expressions.

### Commands

`config.list`, `config.create {name, copy?}`, `config.delete`, `config.rename`,
`config.activate {name?}` (no name: none), `config.set {configuration, parameter, expr}` /
`{.., feature, suppressed}` / `{.., body, color | appearance}` / `{.., joint, motion?, value}`
(a `null` value clears the cell), `config.table` (rows × columns for the table dialog).
`export {path, configuration}` exports one configuration without switching the session, and
the CLI takes `--configuration NAME` on `export`, `convert`, `render` and `run`.

### UI

The browser shows a "Configurations" folder with one row per configuration (the active one
highlighted; click to activate, right-click to rename or delete), and the table dialog edits
every cell in a grid: rows are configurations, columns are overrides, an empty cell means
"default".

## Base features and history off

### Data

`FeatureKind::BaseFeature { bodies, edits }` (`crates/model/src/direct.rs`) is a timeline
container for non-parametric geometry:

- `bodies`: frozen bodies stored as OCCT BRep text with their original `BodyId` and name, so
  components and later features that name them keep working.
- `edits`: direct edits replayed in order on the bodies that exist at that point of the
  timeline: `move_faces {faces, translate}`, `press_pull {faces, distance}` and
  `delete_faces {faces}`. Values are plain numbers in millimetres, not expressions: a base
  feature does not respond to parameters. Faces are ordinary `FaceRef`s, so they are re-found
  after upstream changes like any reference.

`Design::history_off` (serde default `false`) records the "Capture design history" setting.

### Behaviour

- **Turning history off** (`design.history {capture: false}`) collapses the timeline the way
  Fusion does: every body as it is now is frozen into one base feature, and the features that
  built them (sketches, joints, construction geometry, ...) are removed. Body ids, names,
  components and display settings are kept. It is one undo step.
- **Direct edits** (`direct.move_face`, `direct.press_pull`, `direct.delete_face`) go into the
  open base feature: the base feature just before the timeline marker. With none open, a new
  one is added at the marker, so consecutive direct edits collect in one container. With
  history off that is the base feature the timeline was collapsed into (parametric features
  added meanwhile still go in the timeline after it).
- `base.start` adds an empty base feature, `base.edit {feature}` rolls the marker back to just
  after an existing one (so direct edits go into it and later features are suspended), and
  `base.finish` moves the marker back to the end.
- **Turning history back on** keeps the base feature as the first timeline entry; new
  parametric features follow it.

Move face moves flat faces: the part of the move along each face's normal is applied (a move
within the face's own plane changes nothing and is reported as a warning). Curved faces are
offset with press-pull. Sheet metal information is not kept when a body is frozen.
