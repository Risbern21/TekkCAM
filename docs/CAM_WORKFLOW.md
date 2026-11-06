# 3D CAM workflow and post processing

## Supported inputs and strategies

STEP import uses `oxideav-step` 0.0.1 under the MIT license. The reader states support for AP203, AP203e2, AP214, and AP242. AP214 is the only full-file import covered by the bundled STEP fixture; see [STEP_IMPORT.md](STEP_IMPORT.md) for unit handling, assembly placement, and tessellation limitations.

The current router strategies are stock-bounded raster roughing, raster/parallel finishing, rectangular contour-parallel finishing, triangle-intersection waterlines, STEP tessellated-face profiles, and 2D isolation, pocket, contour, and drilling operations. Tools support flat, ball-nose, and bull-nose definitions. Ball compensation is sampled for raster and contour finishing; waterline compensation and bull-nose finishing are not implemented.

## Typical workflow

1. Import a STEP model and review its import notes and units.
2. Switch to the isometric view. Configure stock size and origin, work origin, Z orientation, safe Z, clearance, and a work offset in the Properties panel.
3. Add a CAM operation, select its tool, and generate its toolpath. Use the operation list's Move up/Move down controls to set machining order.
4. Inspect the color-coded rapid and cutting moves. Select an operation to see whether its path is generated. Generate G-code to open the editor, review the output, then export it.
5. Optionally enable View → Stock removal preview (sampled) and run the G-code simulator to see the moving cutter marker.

## G-code posts

The console offers GRBL and LinuxCNC output. Both posts use millimeters, absolute coordinates, the XY plane, units-per-minute feed, and the configured G54–G59 work coordinate system. Toolpath coordinates are translated by subtracting the configured work origin; safe retracts use the setup's safe Z in work coordinates. LinuxCNC output emits numbered tool changes (`Tn M6`) for each distinct tool in first-use order. GRBL output emits a manual tool-change comment and `M0` when the tool changes; after installing the tool, resume with the machine's cycle-start control or the UI's `Resume / Cycle start (~)` button. Both posts stop the spindle between operations and at program end.

Machine travel limits are optional project setup values in the selected work-coordinate system. When enabled, generation checks the safe Z and every generated rapid/feed/drill target. XY arcs are sampled at up to 5-degree intervals. A failed check blocks G-code generation. Limits are tool-center coordinates; the checker does not know the machine's actual stored work-offset values or current position. Without configured limits, the generated header says that travel bounds were not verified.

## Stock-removal preview accuracy

The optional preview is a 64 by 64 height field over the configured stock footprint. It sweeps each generated feed, arc, and drill path with the operation's circular cutter footprint and interpolates the cut depth. Arc curves are sampled in 5-degree increments. The XY grid spacing is stock width/64 by stock height/64, with a half-cell-diagonal allowance around the cutter footprint. The display is a visual estimate, not a solid model or a material-physics simulation: it omits vertical walls, fixtures, clamps, holder collisions, chip evacuation, tool deflection, and engagement between samples. It does not update or alter the source model or generated toolpaths.

## Controller and machine limitations

The posts currently target the application's GRBL and LinuxCNC dialect choices. GRBL manual tool-change pauses require operator supervision. The optional travel check verifies programmed tool-center coordinates against user-entered bounds; it cannot validate controller configuration, physical homing, actual work offsets, machine state, spindle behavior, clamps, or emergency-stop hardware. Review the exported G-code and machine setup before running a job. Further path-check limitations are documented in [SAFETY.md](SAFETY.md).

## Fusion-style 2D options

Derived from a feature study of Fusion 360 CAM ([FUSION_CAM_EVIDENCE.md](FUSION_CAM_EVIDENCE.md)):

- **Contour chaining.** Isolation, contour and STEP-profile operations link loose edges (within 0.001 mm) into continuous polylines, mitre the offset at corners (bevel beyond 2x the offset), and cut each chain in one pass without retracting between edges. The first edge of a chain fixes its travel direction, and the offset is to the left of that direction.
- **Ramp entry.** A non-zero ramp angle (0-45 degrees) descends along the chain at the plunge feed instead of plunging vertically, then re-cuts the ramped section at full depth. A chain too short for the requested angle falls back to a vertical plunge. Ramps run along the cut path, not in a lead-in outside the part, so on closed profiles they cut inside the finished contour; use stock to leave or a sacrificial edge where that matters.
- **Peck drilling.** A peck depth expands drilling into feed and retract moves. With a chip-break distance the tool backs out by that distance and continues; without it, it retracts fully to safe Z and re-enters 0.5 mm above the last depth. Dwell is not implemented.
- **Pocket stepover.** Expressed as a fraction of the tool diameter (default 0.8).

Not implemented: lead-in/out arcs, feed reduction on inner corners, adaptive clearing, and the Face, Chamfer2D, Slot and Thread strategies.
