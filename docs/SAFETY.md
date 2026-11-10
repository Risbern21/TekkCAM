# Phase 9 path safety and tolerance limits

tekkCAM generates paths from the imported STEP triangle mesh. STEP imports use the setup's absolute chordal tolerance (0.05 mm by default); changing it affects subsequently imported models. Path simplification uses a 3D Ramer–Douglas–Peucker bound (0.02 mm by default), preserves rapid boundaries, and runs before the path checks.

Roughing uses raster samples for cutter accessibility, then samples feed segments at a spacing bounded by the path tolerance and one quarter of the cutter radius. Cutter access uses the mesh query's center plus eight radial samples. Holder checks use a sampled cylindrical envelope based on flute length and holder clearance. Finishing checks rapid clearance and holder clearance similarly. Flat raster and contour-parallel finishing also run a sampled centerline gouge check. Waterline paths are not checked with that vertical height query because vertical walls make it report false collisions; ball-nose gouge checking is also unsupported. Roughing reports a sampled residual-stock volume and maximum thickness estimate; it is based on an XY raster and sampled cutter-center path, not a stock-removal simulation.

These checks can miss collisions between samples, depend on STEP tessellation quality, and do not prove continuous swept-volume clearance. They do not model clamps, fixtures, spindle/collet geometry beyond the configured cylindrical holder envelope, or material already removed by earlier operations. Optional G-code machine travel checks compare programmed tool-center points and sampled arcs with user-configured work-coordinate bounds; they do not validate the controller's actual offsets, homing, or physical travel. Generated 3D paths show an approximate-check warning in the console.

The optional stock-removal display is a 64 by 64 sampled height-field estimate, not a solid stock simulation. It approximates cutter sweeps from generated paths and omits walls, fixtures, and material behavior. See [CAM_WORKFLOW.md](CAM_WORKFLOW.md) for its resolution and workflow.

Roughing and finishing make disconnected cuts with retracts to the configured clearance height. When the run is long enough, entry and exit ramps use the configured ramp angle; short runs retain a direct plunge/retract. Stepdown and stepover remain bounded by the project setup.

## Probing

Probing commands move the physical machine. Confirm the probe is wired and configured, the input is not already triggered, the selected direction is correct, the starting position is clear, and the maximum travel and feed are safe. GRBL probing uses G38.2 and expects a valid probe-result report; an unsuccessful trigger aborts the map and partial data is discarded. LinuxCNC programs are exported for the operator to run in LinuxCNC, and recorded results must be imported and reviewed. tekkCAM does not directly connect to LinuxCNC or independently verify probe wiring or polarity.

Surface compensation uses bilinear interpolation between measured points relative to the first map point. It cannot represent features smaller than the sampling grid, and arcs are corrected at their endpoint rather than continuously subdivided. Review the compensated path, machine limits, and clearance, and perform an air cut before machining. A probe map is not a collision guarantee.
