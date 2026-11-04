# Frolic

An interactive 3D magnetic field simulator. Place permanent magnets, drag and rotate them,
and watch the field lines update in real time.

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

The side panel loads presets, adds magnets, edits size, pose and magnetization
(strength Br and direction), and tunes the field-line display.

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
    presets.rs     example scenes
    devtools.rs    FROLIC_PRESET=<n> and FROLIC_SCREENSHOT=<png> hooks for automated checks
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
