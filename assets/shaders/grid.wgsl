// grid.wgsl — Infinite grid for the tekkCAM viewport
// Renders a full-screen quad using a built-in vertex trick (no vertex buffer).
// Grid lines are drawn analytically in the fragment shader.

struct GridUniforms {
    view_proj: mat4x4<f32>,
    viewport_size: vec2<f32>,
    zoom: f32,
    _pad: f32,
    minor_spacing: f32,   // world units (mm) between minor lines
    major_spacing: f32,   // world units (mm) between major lines
    _pad2: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u: GridUniforms;

// ── Vertex stage ─────────────────────────────────────────────────────────────
// Emit a full-screen triangle from built-in vertex IDs (no vertex buffer).
struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       screen_pos: vec2<f32>,  // pixel coords
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOut {
    // Positions for a triangle that covers the whole NDC [-1,1] square
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    let ndc = positions[vid];
    var out: VertexOut;
    out.clip_pos  = vec4(ndc, 0.0, 1.0);
    // Screen-space pixel position
    out.screen_pos = (ndc * 0.5 + 0.5) * u.viewport_size;
    return out;
}

// ── Fragment stage ────────────────────────────────────────────────────────────
// Compute world-space coords from screen pos and draw grid lines.

/// Compute anti-aliased line strength given distance to nearest line.
fn grid_line(world_coord: f32, spacing: f32, line_width_world: f32) -> f32 {
    let wrapped = abs(fract(world_coord / spacing + 0.5) - 0.5) * spacing;
    let fade = smoothstep(line_width_world, 0.0, wrapped);
    return fade;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Convert screen pixel to world coords (inverse of camera transform)
    // Screen origin: top-left. World Y is up.
    let screen_pos = in.screen_pos;

    // Reconstruct pan from view_proj (col-major):
    // The 4th column of view_proj contains the translation in NDC:
    //   tx = -pan.x * sx,  ty = -pan.y * sy
    // where sx = 2*zoom/w, sy = 2*zoom/h
    let sx = u.view_proj[0][0];  // 2*zoom/w
    let sy = u.view_proj[1][1];  // 2*zoom/h
    let tx = u.view_proj[3][0];  // -pan.x * sx
    let ty = u.view_proj[3][1];  // -pan.y * sy

    let pan_x = -tx / sx;
    let pan_y = -ty / sy;

    // World coords: pan is the world point at the viewport centre.
    // screen_pos is derived from NDC, so its Y already points up.
    let centred = screen_pos - u.viewport_size * 0.5;
    let wx = centred.x / u.zoom + pan_x;
    let wy = centred.y / u.zoom + pan_y;

    // Adaptive line width (thinner at low zoom)
    let px_per_mm = u.zoom;
    let minor_line_w = 0.5 / px_per_mm;  // 0.5px wide in world units
    let major_line_w = 1.2 / px_per_mm;

    // Minor grid lines (1 mm spacing, fade out when zoomed out)
    var minor_alpha = 0.0;
    if px_per_mm > 4.0 {
        let fade_factor = clamp((px_per_mm - 4.0) / 20.0, 0.0, 1.0);
        let lx = grid_line(wx, u.minor_spacing, minor_line_w);
        let ly = grid_line(wy, u.minor_spacing, minor_line_w);
        minor_alpha = max(lx, ly) * fade_factor * 0.25;
    }

    // Major grid lines (10 mm spacing)
    let mlx = grid_line(wx, u.major_spacing, major_line_w);
    let mly = grid_line(wy, u.major_spacing, major_line_w);
    let major_fade = clamp((px_per_mm * u.major_spacing - 3.0) / 10.0, 0.0, 1.0);
    let major_alpha = max(mlx, mly) * 0.55 * major_fade;

    // Origin axes (stronger)
    let origin_w = 2.0 / px_per_mm;
    let ax = grid_line(wy, 1e9, origin_w);  // X axis (y == 0)
    let ay = grid_line(wx, 1e9, origin_w);  // Y axis (x == 0)

    // Compose colours
    let minor_col  = vec4(0.3, 0.3, 0.4, minor_alpha);
    let major_col  = vec4(0.4, 0.45, 0.6, major_alpha);
    let x_axis_col = vec4(0.85, 0.25, 0.25, ax * 0.9); // red  — X axis
    let y_axis_col = vec4(0.25, 0.75, 0.35, ay * 0.9); // green — Y axis

    var col = minor_col;
    col = mix(col, major_col,  major_alpha);
    col = mix(col, x_axis_col, x_axis_col.a);
    col = mix(col, y_axis_col, y_axis_col.a);

    return col;
}
