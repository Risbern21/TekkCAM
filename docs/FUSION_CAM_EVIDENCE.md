# Fusion 360 CAM: evidence and tekkCAM feature mapping

This note records what was observed in a locally installed Autodesk Fusion 360
build, how it was observed, and which tekkCAM features were derived from it.
It is a feature-level study. **No Fusion code was decompiled, copied or ported.**
tekkCAM's algorithms are independent implementations written from the observable
parameter names and documented behaviour below.

Install inspected: `%LOCALAPPDATA%\Autodesk\webdeploy\production\39e8a3d5e919c70b05ef3ebfe9c6bfc8d9ee001e`
(CAM files under `Applications\CAM360`). Date of inspection: 2026-11-05.

## Method

| Evidence type | Source | Tool |
|---|---|---|
| Observed: public API typings (strategy list, operation model) | `API\TypeScript\@adsk\fusion\cam.d.ts` | file read |
| Observed: strategy parameter schemas | `Applications\CAM360\Data\customstrategy\metaparameters*.json` | file read + Node JSON parse |
| Observed: bundled data (posts, machines, templates, tool libraries) | `Applications\CAM360\Data\*` | directory listing |
| Observed: native module export tables (class names only) | `Applications\CAM360\IronNC10.dll`, `NaNeuCAM10.dll`, `IronGeometry10.dll` | REA `inspect-native-exports` (rea-agents 6.8.0) |
| Inferred: architecture from class names | the export tables above | reasoning, not execution |

REA Evidence IDs (static, PE export directory only; nothing was executed):

| Module | SHA-256 | Exports | Evidence ID |
|---|---|---|---|
| IronNC10.dll | `0427783e…e947` | 5118 | `ev_14c996ab6de535f1c57f8a6eee3931252dda296f1b391674a0dcf3f9400d6daf` |
| NaNeuCAM10.dll | `cd8d065f…368c` | 4233 | `ev_2955020ba56fd85a459661aa52c2e04501f0f383387dc68698a32adba74decb5` |
| IronGeometry10.dll | `008f6f09…fb7` | 1659 | `ev_b07fd01d8422ca9b75ae8108df5a6a018f4777bc7850c89f45e5f1393f1c6a02` |

Unknowns: how Fusion's strategies compute their paths is not established by any
of this evidence; export names show structure, not algorithms.

## Findings

### 1. Strategy catalogue (observed)

`cam.d.ts` lines 8666-8783 (`OperationStrategyTypes`, marked deprecated in favour of string names) enumerates:

- **2D:** AdaptiveClearing2D, Pocket2D, Face, Contour2D, Slot, Trace, Thread, Bore, Circular, Engrave, Chamfer2D
- **3D roughing:** AdaptiveClearing, PocketClearing
- **3D finishing:** Parallel, Contour, Ramp, Horizontal, Pencil, Scallop, Spiral, Radial, MorphedSpiral, Projection, SteepAndShallow, Flow/Flow2, Morph, RestFinishing
- **Multi-axis:** RotaryFinishing, MultiAxisContour, MultiAxisMorph, Swarf
- **Hole making:** Drilling (drill, tap, counterbore, countersink, ...)
- **Other:** Jet2D, Turning*, probing/inspection

tekkCAM already covers a subset (Pocket, Contour, Drill, Roughing, Parallel,
Contour and Waterline finishing). Not covered: Face, Chamfer2D, Slot, Bore/Circular,
Thread, Pencil, Scallop, Spiral/Radial, Rest, Adaptive.

### 2. Per-operation parameter model (observed)

`metaparameters_lemoine.json` (strategies `drill`, `chamfer2d`) and
`metaparameters_mw.json` (`three_plus_two`) list the parameter ids of each
operation, grouped as tool / geometry / heights / passes / linking / cycle tabs
(`metaparameters_tabs.json`). Relevant ids:

- **Passes:** `tolerance`, `chainingTolerance`, `stepover`, `maximumStepdown`, `stockToLeave`, `compensationType`, `doFinishingPasses`, `finishingStepover`, `direction`
- **Ramping:** `doRamp`, `rampType`, `rampAngle`, `maximumRampLength`, `minimumRampLength`, `helicalRampDiameter`
- **Leads:** `doLeadIn`, `entry_radius`, `entry_sweep`, `doLeadOut`, `exit_radius`, `exit_sweep`, `transitionType`
- **Linking:** `retractionPolicy`, `safeDistance`, `stayDownDistance`, `liftHeight`, `highFeedrate`
- **Heights:** clearance / retract / feed / top / bottom height groups, each with mode + offset
- **Drilling cycle:** `cycleType`, `peckingDepth`, `peckingDepthReduction`, `minimumPeckingDepth`, `accumulatedPeckingDepth`, `chipBreakDistance`, `dwellingPeriod`, `breakThroughDistance`, `breakThroughFeedrate`, `backBoreDistance`
- **Feed optimisation:** `useFeedOptimization`, `reducedFeedrate`, `reducedFeedRadius`, `reduceOnlyInnerCorners`, `surfaceSpeedOnArcs`

### 3. Toolpath pipeline architecture (inferred from export names)

`IronNC10.dll` exports a stream-based pipeline: `ToolpathStream` with composable
filters (`ToolpathStreamFeedOptimization`, `ToolpathStreamEngagementFeedFilter`,
`ToolpathStreamRadiusCompensationParams`, `ToolpathAxisSubstitution`,
`ToolpathStreamValidator`, `ToolpathStreamTransformationFilter`), binders between
toolpath and NC streams (`ToolpathToNCStreamBinder`, `NCStreamToToolpathStreamBinder`),
and simulation types (`SimulationCollision`, `SimulationIllegalCut`). Interpretation:
path generation, post-generation filters (feed optimisation, validation) and
NC output are separate stages. `NaNeuCAM10.dll` adds the post editor and
tool-library managers (`PostEditor`, `PostEditSession`, `ToolLibraries`).

### 4. Bundled data (observed)

~100 post processors as editable `.cps` scripts (including `grbl.cps`, `linuxcnc.cps`,
`mach3mill.cps`), ~40 machine definitions (generic 3-axis, 5-axis variants, lathe,
mill-turn), tool and holder libraries as JSON, hole-making templates
(spot-drill + drill + ream / tap / counterbore), and a setup-sheet template.

## Mapping to tekkCAM

| Fusion capability (evidence) | tekkCAM status before | Change in this branch |
|---|---|---|
| `chainingTolerance`: open/closed contours chained from edges | Contour treated each edge independently (retract between every segment) | `cam::chain` links edges into polylines within a tolerance; contour/profile cuts continuous chains with mitred joins |
| `doRamp` / `rampAngle` | vertical plunge only | `ramp_angle_deg` on operations; ramps down along the chain |
| Drilling `peckingDepth`, `chipBreakDistance` | single plunge | `peck_depth_mm`, `chip_break_mm`; expands to feed/retract moves (works on GRBL without canned cycles) |
| `stepover` | hard-coded 0.8 x tool diameter for pocket | `stepover_ratio` (default 0.8, serialised) |
| Not implemented here | | Lead-in/out arcs, feed optimisation, Face/Chamfer2D/Slot/Thread, dwell, post scripts, adaptive clearing |

Compatibility: all new `Operation` fields are `#[serde(default)]`; their
defaults reproduce earlier behaviour (no ramp, no peck, 0.8 stepover), so existing
project files load unchanged.
