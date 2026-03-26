// Fragment shader: SDF-based 2D rendering pipeline.
//
// Vertex stage: full-screen triangle (3 vertices, no vertex buffer).
// Fragment stage: iterate over DrawCmd nodes, evaluate SDF primitives,
// composite with alpha blending front-to-back.
//
// SDF primitives: rounded box, circle, line segment.
// Each DrawCmd specifies type via params.x:
//   0 = Box, 1 = Slab (rounded rect), 2 = Circle, 3 = Line, 4 = Text

// ── DrawCmd (matches compute shader output and gpu.rs) ──

struct DrawCmd {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    params: vec4<f32>,   // [ty, radius, softness, slot]
};

struct RenderHeader {
    cmd_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

struct GpuUniforms {
    time_delta: vec4<f32>,
    resolution: vec4<f32>,
    mouse: vec4<f32>,
    theme: vec4<f32>,
    vec4_bank: array<vec4<f32>, 16>,
    vec3_bank: array<vec4<f32>, 16>,
    scalar_bank: array<vec4<f32>, 4>,
    int_bank: array<vec4<i32>, 4>,
    zero_page: array<vec4<u32>, 16>,
};

// ── Bindings ──

@group(0) @binding(0) var<uniform> uniforms: GpuUniforms;
@group(0) @binding(1) var<storage, read> draw_cmds: array<DrawCmd>;
@group(0) @binding(2) var<storage, read> header: RenderHeader;

// ── Vertex shader: full-screen triangle ──

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Generate a full-screen triangle from 3 vertices (no vertex buffer needed).
    // Vertex 0: (-1, -1), Vertex 1: (3, -1), Vertex 2: (-1, 3)
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: map clip space to pixel coordinates
    out.uv = vec2<f32>(
        (x + 1.0) * 0.5 * uniforms.resolution.x,
        (1.0 - (y + 1.0) * 0.5) * uniforms.resolution.y
    );
    return out;
}

// ── SDF primitives ──

/// Signed distance to an axis-aligned box centered at origin.
fn sd_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let d = abs(p) - half_size;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

/// Signed distance to a rounded box (box with corner radius).
fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    return sd_box(p, half_size - vec2<f32>(r)) - r;
}

/// Signed distance to a circle centered at origin.
fn sd_circle(p: vec2<f32>, radius: f32) -> f32 {
    return length(p) - radius;
}

/// Signed distance to a line segment from a to b with given thickness.
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, thickness: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - thickness;
}

/// Alpha-blend src over dst (premultiplied alpha).
fn blend_over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let out_a = src.a + dst.a * (1.0 - src.a);
    if out_a < 0.001 {
        return vec4<f32>(0.0);
    }
    let out_rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}

// ── Fragment shader ──

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.uv;
    let is_dark = uniforms.theme.x;

    // Background color based on theme
    var bg: vec3<f32>;
    if is_dark > 0.5 {
        bg = vec3<f32>(0.1, 0.1, 0.12);
    } else {
        bg = vec3<f32>(0.95, 0.95, 0.97);
    }
    var result = vec4<f32>(bg, 1.0);

    let count = header.cmd_count;

    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let cmd = draw_cmds[i];
        let ty = u32(cmd.params.x);
        let center = cmd.pos + cmd.size * 0.5;
        let p = pixel - center;

        var d: f32 = 1e6;
        var shadow_d: f32 = 1e6;

        switch ty {
            case 0u: { // Box
                let half = cmd.size * 0.5;
                d = sd_box(p, half);
                shadow_d = sd_box(p - vec2<f32>(2.0, 3.0), half);
            }
            case 1u: { // Slab (rounded rect)
                let half = cmd.size * 0.5;
                let radius = cmd.params.y;
                d = sd_rounded_box(p, half, radius);
                shadow_d = sd_rounded_box(p - vec2<f32>(2.0, 3.0), half, radius);
            }
            case 2u: { // Circle
                let radius = cmd.params.y;
                d = sd_circle(p, radius);
                shadow_d = sd_circle(p - vec2<f32>(1.5, 2.0), radius);
            }
            case 3u: { // Line
                let half_len = cmd.size.x * 0.5;
                let a = vec2<f32>(-half_len, 0.0);
                let b = vec2<f32>(half_len, 0.0);
                d = sd_segment(p, a, b, cmd.size.y * 0.5);
            }
            case 4u: { // Text placeholder — render as colored box
                let half = cmd.size * 0.5;
                d = sd_box(p, half);
            }
            default: {
                // Unknown type — skip
            }
        }

        // Shadow pass (subtle drop shadow for depth)
        if ty <= 2u {
            let shadow_alpha = 0.15 * (1.0 - smoothstep(-1.0, 4.0, shadow_d));
            let shadow_color = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
            result = blend_over(result, shadow_color);
        }

        // Shape fill with anti-aliased edge
        let fill_alpha = cmd.color.a * (1.0 - smoothstep(-0.5, 0.5, d));
        if fill_alpha > 0.001 {
            let shape_color = vec4<f32>(cmd.color.rgb, fill_alpha);
            result = blend_over(result, shape_color);
        }
    }

    return result;
}
