// Fragment shader: SDF-based 2D rendering pipeline.
//
// Vertex stage: full-screen triangle (3 vertices, no vertex buffer).
// Fragment stage: iterate over DrawCmd nodes, evaluate SDF primitives,
// composite with alpha blending front-to-back.
//
// SDF primitives: rounded box, circle, line segment.
// Each DrawCmd specifies type via params.x:
//   0 = Box, 1 = Slab (rounded rect), 2 = Circle, 3 = Line, 4 = Text, 8 = MSDF Text

// ── DrawCmd (matches compute shader output and gpu.rs) ──

struct DrawCmd {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    params: vec4<f32>,   // [ty, radius, anim_idx, slot]
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
    // Font descriptor: [glyph_w, glyph_h, first_cp, last_cp]
    font: vec4<u32>,
};

struct Anim {
    freq: f32,
    duty: f32,
    enable_ref: u32,
    _pad: u32,
};

struct GpuTexture {
    width: u32,
    height: u32,
    layer: u32,
    flags: u32,
};

// ── Bindings ──

@group(0) @binding(0) var<uniform> uniforms: GpuUniforms;
@group(0) @binding(1) var<storage, read> draw_cmds: array<DrawCmd>;
@group(0) @binding(2) var<storage, read> header: RenderHeader;
@group(0) @binding(3) var<storage, read> anim_bank: array<Anim>;
@group(0) @binding(4) var<storage, read> glyph_bitmap: array<u32>;
@group(0) @binding(5) var<storage, read> char_buffer: array<u32>;
@group(0) @binding(6) var tex_array: texture_2d_array<f32>;
@group(0) @binding(7) var tex_sampler: sampler;
@group(0) @binding(8) var<storage, read> texture_bank: array<GpuTexture>;

// MSDF atlas for high-quality text rendering
@group(0) @binding(9)  var msdf_atlas: texture_2d<f32>;
@group(0) @binding(10) var msdf_sampler: sampler;

// Per-glyph atlas lookup: [glyph_id, atlas_xy_packed, atlas_wh_packed, 0, bearing_x_bits, bearing_y_bits, 0, 0]
@group(0) @binding(11) var<storage, read> glyph_table: array<vec4<u32>>;

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
    // UV: map clip space to logical pixel coordinates
    // resolution.xy = physical pixels, resolution.z = scale factor
    let scale = max(uniforms.resolution.z, 1.0);
    out.uv = vec2<f32>(
        (x + 1.0) * 0.5 * uniforms.resolution.x / scale,
        (1.0 - (y + 1.0) * 0.5) * uniforms.resolution.y / scale
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

/// MSDF median: the middle value of RGB channels gives the signed distance.
fn msdf_median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
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

    // Ribbon state tracking
    var in_ribbon: bool = false;
    var ribbon_clip_min: vec2<f32> = vec2<f32>(0.0);
    var ribbon_clip_max: vec2<f32> = vec2<f32>(0.0);
    var ribbon_scroll: vec2<f32> = vec2<f32>(0.0);

    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let cmd = draw_cmds[i];
        let ty = u32(cmd.params.x);

        // Handle ribbon markers first (no geometry)
        if ty == 6u { // RIBBON_BEGIN
            in_ribbon = true;
            ribbon_clip_min = cmd.pos;
            ribbon_clip_max = cmd.pos + cmd.size;
            // Read scroll offset from scalar_bank[slot]
            let slot = u32(cmd.params.y);
            let pack = slot / 4u;
            let comp = slot % 4u;
            let scroll_val = uniforms.scalar_bank[min(pack, 3u)][min(comp, 3u)];
            let dir = cmd.params.z;
            if dir > 0.5 {
                ribbon_scroll = vec2<f32>(0.0, scroll_val);
            } else {
                ribbon_scroll = vec2<f32>(scroll_val, 0.0);
            }
            continue;
        }
        if ty == 7u { // RIBBON_END
            in_ribbon = false;
            ribbon_scroll = vec2<f32>(0.0);
            continue;
        }

        // Ribbon clipping: skip this command for pixels outside viewport
        if in_ribbon {
            if pixel.x < ribbon_clip_min.x || pixel.x >= ribbon_clip_max.x ||
               pixel.y < ribbon_clip_min.y || pixel.y >= ribbon_clip_max.y {
                continue;
            }
        }

        // Compute effective pixel (with scroll offset for ribbon children)
        var effective_pixel = pixel;
        if in_ribbon {
            effective_pixel = pixel - ribbon_scroll;
        }

        let center = cmd.pos + cmd.size * 0.5;
        let p = effective_pixel - center;

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
            case 4u: { // Text — render using font bitmap atlas
                let glyph_w = uniforms.font.x;
                let glyph_h = uniforms.font.y;
                let first_cp = uniforms.font.z;

                if glyph_w > 0u && glyph_h > 0u {
                    let packed = bitcast<u32>(cmd.params.w);
                    let char_offset = packed >> 16u;
                    let char_count = packed & 0xFFFFu;

                    let text_x = cmd.pos.x;
                    let text_y = cmd.pos.y;
                    let text_size = cmd.size.y;
                    let scale_f = max(text_size / f32(glyph_h), 1.0);
                    let advance = f32(glyph_w + 1u) * scale_f;

                    for (var ci: u32 = 0u; ci < char_count; ci = ci + 1u) {
                        let cp = char_buffer[char_offset + ci];
                        let glyph_idx = clamp(cp, first_cp, uniforms.font.w) - first_cp;

                        let char_x = text_x + f32(ci) * advance;
                        let local_x = effective_pixel.x - char_x;
                        let local_y = effective_pixel.y - text_y;

                        if local_x >= 0.0 && local_x < f32(glyph_w) * scale_f &&
                           local_y >= 0.0 && local_y < f32(glyph_h) * scale_f {
                            let gx = u32(local_x / scale_f);
                            let gy = u32(local_y / scale_f);
                            let row_byte = glyph_bitmap[glyph_idx * glyph_h + gy];
                            let bit = glyph_w - 1u - gx;
                            if (row_byte & (1u << bit)) != 0u {
                                d = -1.0;
                                break;
                            }
                        }
                    }
                }
            }
            case 5u: { // Texture — sample from texture array
                let tex_idx = u32(cmd.params.y);
                let half = cmd.size * 0.5;
                if abs(p.x) < half.x && abs(p.y) < half.y {
                    let uv = (p + half) / cmd.size;
                    let tex = texture_bank[tex_idx];
                    let color_sample = textureSample(tex_array, tex_sampler, uv, i32(tex.layer));
                    let blend_alpha = color_sample.a * cmd.color.a;
                    if blend_alpha > 0.001 {
                        let tinted = vec4<f32>(color_sample.rgb * cmd.color.rgb, blend_alpha);
                        result = blend_over(result, tinted);
                    }
                }
            }
            case 8u: { // MSDF Text — render using multi-channel signed distance field atlas
                // params.y = px_range (MSDF distance range in atlas pixels)
                // params.w = packed (char_offset << 16 | char_count)
                // char_buffer entries: packed [16b glyph_table_index | 16b x_advance_fixed4]
                let px_range = max(cmd.params.y, 2.0);
                let packed = bitcast<u32>(cmd.params.w);
                let char_offset = packed >> 16u;
                let char_count = packed & 0xFFFFu;

                let text_x = cmd.pos.x;
                let text_y = cmd.pos.y;
                let text_h = cmd.size.y; // font size in screen pixels

                let atlas_dim = vec2<f32>(textureDimensions(msdf_atlas));

                var cursor_x: f32 = 0.0;

                for (var ci: u32 = 0u; ci < char_count; ci = ci + 1u) {
                    let entry = char_buffer[char_offset + ci];
                    let gt_idx = entry >> 16u;
                    // Delta from standard advance: biased by 2048, 1/16 px resolution
                    let delta_biased = f32(entry & 0xFFFFu);
                    let delta_px = (delta_biased - 2048.0) / 16.0;

                    // Glyph table: 2 × vec4<u32> per entry
                    let g0 = glyph_table[gt_idx * 2u];
                    let g1 = glyph_table[gt_idx * 2u + 1u];

                    let atlas_gx = f32(g0.y & 0xFFFFu);
                    let atlas_gy = f32(g0.y >> 16u);
                    let atlas_gw = f32(g0.z & 0xFFFFu);
                    let atlas_gh = f32(g0.z >> 16u);

                    // Standard advance + bearings, normalized to em square
                    let advance_x_norm = bitcast<f32>(g1.x);
                    let bearing_x_norm = bitcast<f32>(g1.y);
                    let bearing_y_norm = bitcast<f32>(g1.z);

                    // Reconstruct actual advance: standard * font_size + delta
                    let advance_px = advance_x_norm * text_h + delta_px;

                    // Scale from atlas pixels to screen pixels
                    let glyph_scale = text_h / atlas_gh;
                    let screen_h = atlas_gh * glyph_scale;

                    // Glyph position
                    let gx = text_x + cursor_x;
                    let baseline_y = text_y + text_h * 0.8;
                    let gy = baseline_y - text_h; // top of em square

                    let local_x = effective_pixel.x - gx;
                    let local_y = effective_pixel.y - gy;

                    // Hit-test: advance width horizontally, em square vertically
                    if local_x >= 0.0 && local_x < advance_px &&
                       local_y >= 0.0 && local_y < text_h * 1.2 {
                        // Map screen position to atlas UV.
                        // autoframe maps glyph bbox to fill the atlas cell.
                        // X: proportional map, no mirror
                        let norm_x = local_x / (advance_x_norm * text_h);
                        let atlas_local_x = norm_x * atlas_gw;
                        // Y: proportional map, flipped (font Y up → screen Y down)
                        let norm_y = local_y / (text_h * 1.2);
                        let atlas_local_y = (1.0 - norm_y) * atlas_gh;

                        if atlas_local_x >= 0.0 && atlas_local_x < atlas_gw &&
                           atlas_local_y >= 0.0 && atlas_local_y < atlas_gh {
                            let u = (atlas_gx + atlas_local_x) / atlas_dim.x;
                            let v = (atlas_gy + atlas_local_y) / atlas_dim.y;

                            let sample = textureSample(msdf_atlas, msdf_sampler, vec2<f32>(u, v));
                            let sd = msdf_median(sample.r, sample.g, sample.b);

                            // Anti-aliased edge.
                            // MSDF as mask: sd > 0.5 = inside glyph
                            let alpha = clamp(px_range * (sd - 0.5) + 0.5, 0.0, 1.0);
                            if alpha > 0.01 {
                                let glyph_color = vec4<f32>(cmd.color.rgb, cmd.color.a * alpha);
                                result = blend_over(result, glyph_color);
                            }
                        }
                    }

                    cursor_x += advance_px;
                }
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

        // Animation: params.z = anim_bank index (0 = none, 1+ = AnimBank[idx-1])
        var anim_alpha: f32 = 1.0;
        let anim_idx = u32(cmd.params.z);
        let has_anim = step(0.5, f32(anim_idx));
        if has_anim > 0.0 {
            let a = anim_bank[max(anim_idx, 1u) - 1u];

            let has_enable = step(0.5, f32(a.enable_ref));
            let slot = a.enable_ref & 0xFFFFu;
            let pack = slot / 4u;
            let comp = slot % 4u;
            let bank_val = f32(uniforms.int_bank[min(pack, 3u)][min(comp, 3u)]);
            let enabled = mix(1.0, bank_val, has_enable);

            let time_s = uniforms.time_delta.x / 1000.0;
            let phase = sin(time_s * a.freq * 6.283185);
            let threshold = 1.0 - a.duty * 2.0;
            let pulse = smoothstep(0.0, 0.1, phase - threshold);

            let has_freq = step(0.001, a.freq);
            anim_alpha = mix(enabled, enabled * pulse, has_freq);
        }

        // Shape fill with anti-aliased edge (skip for texture and MSDF text — they blend internally)
        if ty != 5u && ty != 8u {
            let fill_alpha = cmd.color.a * anim_alpha * (1.0 - smoothstep(-0.5, 0.5, d));
            if fill_alpha > 0.001 {
                let shape_color = vec4<f32>(cmd.color.rgb, fill_alpha);
                result = blend_over(result, shape_color);
            }
        }
    }

    return result;
}
