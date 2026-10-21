# LumenArt

Layered aluminium signs, from artwork to a SendCutSend order: the LumenArt Fusion 360 tool suite
as a FissionOxide add-in. It ships with the app, switched off: turn it on in **File → Plugins**
(or UTILITIES → ADD-INS) and the **LumenArt** tab appears.

The tab lists every step of making a sign in order, numbered on the icons. Steps 1, 2 and 10–14
are FissionOxide's own tools (they came from this suite and are part of the app now); the rest
are LumenArt's.

| Step | Command | What it does |
|------|---------|--------------|
| 1 | Clean Up Sketch | Break projection links, delete constraints and stray points in imported artwork |
| 2 | Remove Duplicate Lines | Delete overlapping curves so nothing is cut twice |
| 3 | Size Sign | Scale the artwork to a width, a height, or the largest size in a box (20 × 20 with 0.25" margin, 36 × 10, custom) |
| 4 | Place Hardware | Keyholes (8" and 16"), #10-32 studs, an E12 socket, LED strip slots, back bracing, part identifier, tab slot and head. Also in Insert → Sketch Library, placed where you click |
| 5 | New Layer | The picked profiles become the next numbered layer (`01`, `02 - Letters` …), 0.063" thick, at its height in the stack |
| 6 | Quick Tab | Click layer edges: a 0.25" tab stub on each, 0.005" proud, one sheet thick |
| 7 | Auto Flange | Every stub grows by the gap to the layer below + 0.025" bend allowance; bend lines drawn in "Bend Lines" sketches |
| 8 | Slot Projector | A 0.200" × 0.085" slot through the layer below every tab pointing down at it |
| 9 | Tab Builder | A locking head (0.110" neck, 0.170" head) on the free end of every flange leg |
| — | Auto Tab | Slot Projector, then Tab Builder |
| 10 | Collect Flat Patterns | Every sheet metal part's flat pattern in one "Cut File" sketch |
| 11 | Check for SendCutSend | Open contours, text, double cuts, features too small, parts too big (also OSH Cut, Ponoko) |
| 12 | Cut Estimate | Area, cut length, pierces and pieces for quoting |
| 13 | Explode Layers | Spread the layers in the view |
| 14 | Render Parts | A picture per part and a part data sheet (powder coat area, finish) |
| 15 | Auto Export | Renders, part data, cut file data and DXF, dimensions and a master JSON in a folder named after the product (`LAP-0002/`) |

Tools find each other's work by size (0.25" × 0.063" tab ends) and by `LumenArt` attributes on
the design, components and features. Auto Flange and Slot Projector remember what they did, so
running them again only handles new tabs. Each run is one undo step.

## Building

```sh
rustup target add wasm32-unknown-unknown   # once
./build.ps1                                 # or ./build.sh: plugin.wasm, then a package check
```

`plugin.wasm` is committed, so the app works without building it. Try a command without the UI:

```sh
fissionoxide-cli plugin run shop.lumenart.signs/new_layer '{"name": "Back"}' --open sign.fox --save sign.fox --dev addins/lumenart
```

The icons are placeholders (grey with the step in yellow); replace `icons/*.png` with the real
ones, keeping the file names. What each command does in detail, and which bugs of the Fusion
version were fixed rather than copied, is in [docs/LUMENART.md](../../docs/LUMENART.md).
