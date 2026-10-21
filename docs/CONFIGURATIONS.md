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
  modelled. Every field has a serde default and is skipped when empty, so existing `.fox` files
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
inside override and rule expressions.

### Rules (driven columns)

A column can be driven by a rule instead of typed cells (`ConfigTable::rules`):

```json
"rules": [
  {"column": {"type": "parameter", "name": "thickness"}, "expr": "if(length > 100 mm, 8 mm, 5 mm)"},
  {"column": {"type": "suppress", "feature": 2}, "expr": "length < 50 mm"},
  {"column": {"type": "appearance", "body": 1000}, "expr": "length > 100 mm",
   "color": [30, 80, 200], "otherwise": null},
  {"column": {"type": "joint", "joint": 12, "motion": "rotation"}, "expr": "if(length > 100 mm, 90 deg, 0 deg)"}
]
```

- The expression is evaluated per row, with that row's parameter values: the design's
  parameters with the row's parameter cells applied (and earlier rules' results; rules run in
  order). Expressions gained comparisons (`<`, `<=`, `>`, `>=`, `==`, `!=`, giving 1 or 0) and
  `if(cond, a, b)`, `and`, `or` and `not` for this.
- A parameter or joint column takes the value (in the parameter's kind, an angle for rotation
  and pitch, a length for slides), stored as text like `8 mm`; a suppression column takes a
  condition (true = suppressed); an appearance column a condition choosing `color` or
  `otherwise` (`null` = no appearance).
- `Design::resolve_configuration` gives a row with its rules applied, and every reader goes
  through it: regeneration (`config::effective`), `body_color`, the table and exports. So the
  rules are evaluated whenever a configuration is activated or exported, and a driven cell
  always shows the current result. A row's own cell in a driven column is ignored (setting a rule
  clears them, and `config.set` refuses to type into a driven column).
- A rule that can't be evaluated for a row (an unknown name, say) leaves that cell at the
  design's own value and is reported: `config.activate` returns `rule_errors`, and the table
  shows the cell as an error with the message.
- Deleting a parameter used in a rule is refused like any other use; deleting a driven
  parameter removes its rule. Rules naming features removed when history is turned off go too.

### Commands

`config.list`, `config.create {name, copy?}`, `config.delete`, `config.rename`,
`config.activate {name?}` (no name: none), `config.set {configuration, parameter, expr}` /
`{.., feature, suppressed}` / `{.., body, color | appearance}` / `{.., joint, motion?, value}`
(a `null` value clears the cell), `config.rule {parameter | feature | body | joint, motion?,
expr, color?, otherwise?}` (`expr: null` removes the rule), `config.table` (rows × columns for
the table dialog; each column carries its `rule`, each row its rule `errors`).
`export {path, configuration}` exports one configuration without switching the session, and
the CLI takes `--configuration NAME` on `export`, `convert`, `render` and `run`.

### UI

The browser shows a "Configurations" folder with one row per configuration (the active one
highlighted; click to activate, right-click to rename or delete), and the table dialog edits
every cell in a grid: rows are configurations, columns are overrides, an empty cell means
"default". Driven columns are marked ƒ and show each row's computed value; the Rules section
below the grid edits their expressions (and an appearance rule's two colours) and adds new ones.

## Configured components

A component inserted from a data hub design (`component.insert`, see the README) can show any
configuration of its source. The link (`HubSource`) records it next to the item and version:

```json
"source": {"item": "i1", "version": 3, "configuration": "Long"}
```

- `component.insert {item, configuration?}` regenerates the source with that configuration
  active (`Design::with_configuration`) and stores the resulting bodies, so the assembly opens
  without the hub as before.
- `component.configuration {component}` lists the source's configurations (at the linked
  version) and the one in use; `{component, configuration}` switches it (`null`: the source as
  modelled). Switching rebuilds the component's bodies from the source in the new
  configuration, reattaching its sub-assembly structure; it is one undo step.
- `component.update_link` keeps the configuration when it brings in a newer version (an error
  if the newer version no longer has it), and `component.links` reports it.
- An unlinked copy (`link: false`, or after `component.break_link`) keeps the bodies of the
  configuration it was inserted in and can't switch.
- In the browser, the Inserted Components folder shows each linked component's configuration
  next to its version, and its right-click Configuration menu switches it.

## Base features and history off

### Data

`FeatureKind::BaseFeature { bodies, edits }` (`crates/model/src/direct.rs`) is a timeline
container for non-parametric geometry:

- `bodies`: frozen bodies stored as OCCT BRep text with their original `BodyId` and name, so
  components and later features that name them keep working.
- `edits`: direct edits replayed in order on the bodies that exist at that point of the
  timeline: `move_faces {faces, translate, rotate?}`, `press_pull {faces, distance}` and
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

Move face (`direct.move_face {faces, translate?, rotate?: {axis, origin?, angle}}`; the edit is
`move_faces {faces, translate, rotate?}`, the rotation applied first) moves faces by any rigid
motion:

- Flat faces that only translate move by the part of the move along each face's normal (a move
  within the face's own plane changes nothing and is reported as a warning), with Offset Face:
  the neighbours keep their surfaces and grow or shrink.
- **One flat face that turns** is tilted onto its moved plane: the body with the face pushed in
  past the new plane is joined with the body with the face pulled out past it and cut off at the
  plane (`Shape::move_faces`, `crates/kernel/src/movefaces.rs`). The neighbours run on along
  their own surfaces to meet the tilted face, as Fusion's do. A face can't turn 85° or more.
- **Curved faces, or a group of faces** (a hole's wall, a boss, a pocket) move as a feature:
  the faces are removed and the body heals (defeaturing), the material they bounded is joined
  back moved and the space they bounded is cut back moved. Before moving, the region's other
  faces (a hole's openings, a boss's foot) are pushed out by as far as the motion carries it
  across them, so a tilted or raised hole still runs through the skin and a tilted boss stays
  seated. A group of flat faces that doesn't bound a feature is tilted face by face.
- A move that would leave the moved faces apart from the rest of the body (a boss lifted off its
  plate) fails with a message instead of making two bodies.

The face references keep pointing at the faces as they were before the move, where replaying
the base feature finds them. Sheet metal information is not kept when a body is frozen.
