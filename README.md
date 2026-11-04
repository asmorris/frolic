# Frolic

An interactive 3D magnetic field simulator. Place permanent magnets, drag and rotate them,
and watch the field lines update in real time.

I called it frolic because I wanted people to enjoy running around and playing in fields.

```sh
cargo run --release      # or `cargo run` (dev builds are optimized enough to be smooth)
cargo test --workspace   # physics tests
```

## Controls

| Input | Action |
| --- | --- |
| Drag a magnet | slide it horizontally |
| Shift + drag a magnet | move it up / down |
| Alt/⌥ + drag a magnet | rotate it |
| Drag empty space | orbit the camera |
| Right-drag, or Shift + drag empty space | pan |
| Scroll / pinch | zoom |
| Q / E | turn the selection 15° |
| F | focus the camera on the selection |
| ⌘D / Ctrl+D | duplicate |
| Delete / Backspace | remove |
| ⌘O / Ctrl+O | open a scene |
| ⌘S / Ctrl+S, ⇧⌘S | save / save as |

The side panel opens example scenes, adds magnets, edits size, pose and magnetization
(strength Br and direction), and tunes the field-line display.

## Saving and sharing scenes

Scenes are saved as human-readable `.frolic.json` files. Open them in any of these ways:

* **Open…** in the panel, or ⌘O
* drag the file onto the window
* `cargo run --release -- path/to/scene.frolic.json`

To share a scene:

* **Send the file.** It's small, readable JSON that diffs nicely in git.
* **Share code.** *Share → Copy share code* puts the whole scene on your clipboard as one
  line of text (`FROLIC1:…`). The recipient pastes it into *Share → Open pasted scene*.
  Codes still work after a chat app wraps them across lines.
* **Add it to the gallery.** The example scenes live in [`scenes/`](scenes/). Open a
  pull request that adds yours there; see [`scenes/README.md`](scenes/README.md).

Saving records the camera view, so a shared scene opens from the same angle. Frolic
tracks unsaved changes (shown as • in the title bar) and asks before anything replaces
them.

### File format

```json
{
  "format": "frolic-scene",
  "version": 1,
  "name": "Two bars, attracting",
  "description": "optional",
  "magnets": [
    {
      "name": "Bar 1",
      "shape": { "type": "block", "size": [4.0, 1.0, 1.0] },
      "position": [-3.0, 0.0, 0.0],
      "rotation": { "yaw_deg": 0.0, "pitch_deg": 0.0, "roll_deg": 0.0 },
      "remanence_tesla": 1.3,
      "magnetization": [1.0, 0.0, 0.0]
    }
  ]
}
```

Shapes are `block` (`size`), `cylinder` (`radius`, `height`; axis along local Z) and
`sphere` (`radius`), all in cm. Rotation is yaw about Y, then pitch about X, then roll
about Z, in degrees. `magnetization` is the direction the north pole points, in the
magnet's own frame. `camera` is optional. Unknown fields are ignored, so files from newer
versions still open where they can, and files with a newer `version` are refused with a
clear message instead of loading incorrectly.

Units: lengths in cm (one grid cell = 1 cm), fields in tesla.

## Layout

```
crates/
  frolic-core/   physics, no graphics dependencies
    analytic.rs    closed-form B-fields: block, cylinder (Derby–Olbert + graded quadrature), sphere, dipole
    shape.rs       primitive geometry: SDF, ray casts, surface sampling
    source.rs      FieldSource trait: the extension point for new kinds of sources
    magnet.rs      uniformly magnetized permanent magnet (+ far-field dipole shortcut)
    scene.rs       FieldScene: an immutable snapshot whose field is the sum of its sources
    fieldlines.rs  flux-weighted seeding + RK4 tracing, run in parallel with rayon
  frolic/        the Bevy 0.16 + egui app
    field.rs       background worker thread, change detection, line rendering
    interaction.rs picking, dragging, rotating, shortcuts
    ui.rs          side panel
    meshgen.rs     magnet meshes with north/south pole colouring
    scene_file.rs  .frolic.json format, validation, share codes
    persistence.rs open/save dialogs, drag-and-drop, unsaved-change tracking
    presets.rs     magnet kinds; embeds the example scenes from scenes/
    devtools.rs    FROLIC_PRESET / FROLIC_SCREENSHOT / FROLIC_SAVE hooks for automated checks
scenes/          example scenes, also the community gallery
```

### Physics notes

* Each magnet's field is **exact**, not sampled on a grid. Fields come from the
  magnetic surface-charge model (blocks, diametric cylinders) or the ideal-solenoid
  current model (axial cylinders). They are tested against brute-force numerical
  integration, ∇·B = 0, ∇×B = 0, and continuity of the normal component of B across
  surfaces.
* Field lines start where flux leaves each surface, spaced so every line carries
  the same flux. Line density therefore shows field strength, as in a textbook diagram.
* Magnets are rigid with fixed magnetization. They don't demagnetize each other yet.

## Roadmap

1. More visualizations: vector-arrow grid, slice-plane heatmap, iron filings.
2. Physics toggle: forces and torques between magnets, then rigid-body motion.
3. Builder: compose primitives into larger magnets, plus voxel sculpting. A voxel is
   a block, so the field stays exact. Later, mesh import via triangle surface charges.
4. Electromagnets: wire segments, loops and solenoids (the same exact cylinder formula),
   iron cores, and AC currents.
5. Full electromagnetics: induction, eddy currents and coupled circuits, built as a
   time-stepping layer that produces successive `FieldScene` snapshots.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
