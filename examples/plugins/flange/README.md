# Flange Tools

An example FissionOxide plugin, written in Rust and compiled to WebAssembly.

- **Bolt Flange** (Flange Tools tab → FLANGES): a round plate with a centre bore and a circle of
  bolt holes, as one undo step. The bolt circle sits halfway between the bore and the rim unless
  you set it. The dialog's hint updates as you change the number of holes.
- **Body Report** (REPORT): writes `bodies.csv` (name, volume, area, size of every body) into the
  folder you pick. Needs the *write-files* permission.

It remembers how many flanges it has made (plugin storage) and reads its CSV header from a
bundled resource.
