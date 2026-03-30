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

// Per-glyph atlas lookup: 2 × vec4<u32> per entry
// g0 = [glyph_id, atlas_xy_packed, atlas_wh_packed, advance_x_bits]
// g1 = [proj_sx_bits, proj_sy_bits, proj_tx_bits, proj_ty_bits]
@group(0) @binding(11) var<storage, read> glyph_table: array<vec4<u32>>;

// ── Vertex shader: full-screen triangle ──

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    let scale = max(uniforms.resolution.z, 1.0);
    out.uv = vec2<f32>(
        (x + 1.0) * 0.5 * uniforms.resolution.x / scale,
        (1.0 - (y + 1.0) * 0.5) * uniforms.resolution.y / scale
    );
    return out;
}

// ── SDF primitives ──

fn sd_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let d = abs(p) - half_size;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    return sd_box(p, half_size - vec2<f32>(r)) - r;
}

fn sd_circle(p: vec2<f32>, radius: f32) -> f32 {
    return length(p) - radius;
}

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

    var bg: vec3<f32>;
    if is_dark > 0.5 {
        bg = vec3<f32>(0.1, 0.1, 0.12);
    } else {
        bg = vec3<f32>(0.95, 0.95, 0.97);
    }
    var result = vec4<f32>(bg, 1.0);

    let count = header.cmd_count;

    var in_ribbon: bool = false;
    var ribbon_clip_min: vec2<f32> = vec2<f32>(0.0);
    var ribbon_clip_max: vec2<f32> = vec2<f32>(0.0);
    var ribbon_scroll: vec2<f32> = vec2<f32>(0.0);

    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let cmd = draw_cmds[i];
        let ty = u32(cmd.params.x);

        if ty == 6u {
            in_ribbon = true;
            ribbon_clip_min = cmd.pos;
            ribbon_clip_max = cmd.pos + cmd.size;
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
        if ty == 7u {
            in_ribbon = false;
            ribbon_scroll = vec2<f32>(0.0);
            continue;
        }

        if in_ribbon {
            if pixel.x < ribbon_clip_min.x || pixel.x >= ribbon_clip_max.x ||
               pixel.y < ribbon_clip_min.y || pixel.y >= ribbon_clip_max.y {
                continue;
            }
        }

        var effective_pixel = pixel;
        if in_ribbon {
            effective_pixel = pixel - ribbon_scroll;
        }

        let center = cmd.pos + cmd.size * 0.5;
        let p = effective_pixel - center;

        var d: f32 = 1e6;
        var shadow_d: f32 = 1e6;

        switch ty {
            case 0u: {
                let half = cmd.size * 0.5;
                d = sd_box(p, half);
                shadow_d = sd_box(p - vec2<f32>(2.0, 3.0), half);
            }
            case 1u: {
                let half = cmd.size * 0.5;
                let radius = cmd.params.y;
                d = sd_rounded_box(p, half, radius);
                shadow_d = sd_rounded_box(p - vec2<f32>(2.0, 3.0), half, radius);
            }
            case 2u: {
                let radius = cmd.params.y;
                d = sd_circle(p, radius);
                shadow_d = sd_circle(p - vec2<f32>(1.5, 2.0), radius);
            }
            case 3u: {
                let half_len = cmd.size.x * 0.5;
                let a = vec2<f32>(-half_len, 0.0);
                let b = vec2<f32>(half_len, 0.0);
                d = sd_segment(p, a, b, cmd.size.y * 0.5);
            }
            case 4u: { // Text — bitmap font atlas
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
            case 5u: { // Texture
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
            case 8u: { // MSDF Text — uniform em-square projection
                //
                // Line box model:
                //   pos.y    = top of the line box
                //   size.y   = line box height
                //   params.z = baseline_frac (baseline position from top, e.g. 0.75)
                //
                // Uniform projection: all glyphs share the same scale mapping
                // em-normalized coords → atlas cell pixels. Stored in glyph table
                // as proj_sx/sy/tx/ty (identical for all entries).
                //
                let px_range = max(cmd.params.y, 2.0);
                let baseline_frac = cmd.params.z;
                let packed = bitcast<u32>(cmd.params.w);
                let char_offset = packed >> 16u;
                let char_count = packed & 0xFFFFu;

                let line_x = cmd.pos.x;
                let line_y = cmd.pos.y;
                let line_h = cmd.size.y;
                let font_size = baseline_frac * line_h;
                let baseline_screen_y = line_y + baseline_frac * line_h;

                let atlas_dim = vec2<f32>(textureDimensions(msdf_atlas));

                var cursor_x: f32 = 0.0;

                for (var ci: u32 = 0u; ci < char_count; ci = ci + 1u) {
                    let entry = char_buffer[char_offset + ci];
                    let gt_idx = entry >> 16u;
                    let delta_biased = f32(entry & 0xFFFFu);
                    let delta_px = (delta_biased - 2048.0) / 16.0;

                    let g0 = glyph_table[gt_idx * 2u];
                    let g1 = glyph_table[gt_idx * 2u + 1u];

                    let atlas_gx = f32(g0.y & 0xFFFFu);
                    let atlas_gy = f32(g0.y >> 16u);
                    let atlas_gw = f32(g0.z & 0xFFFFu);
                    let atlas_gh = f32(g0.z >> 16u);
                    let advance_x_norm = bitcast<f32>(g0.w);

                    // Layout metrics from glyph table
                    let baseline_row = bitcast<f32>(g1.x);  // atlas row of baseline (from top)
                    let px_per_em = bitcast<f32>(g1.y);      // atlas pixels per em
                    let x_margin_px = bitcast<f32>(g1.z);    // left margin in atlas px

                    let advance_px = advance_x_norm * font_size + delta_px;
                    let gx = line_x + cursor_x;

                    // Screen → atlas cell coordinates (simple proportional mapping)
                    // X: screen offset from glyph origin → atlas pixels
                    let screen_dx = effective_pixel.x - gx;
                    let acx = screen_dx / font_size * px_per_em + x_margin_px;

                    // Y: screen offset from baseline → atlas pixels from baseline row
                    // Screen Y increases downward; atlas row increases downward
                    // So screen pixels below baseline → atlas rows below baseline_row
                    let screen_dy = effective_pixel.y - baseline_screen_y;
                    let acy = baseline_row + screen_dy / font_size * px_per_em;

                    // Atlas cell bounds check (prevents sampling adjacent cells)
                    if acx >= 0.0 && acx < atlas_gw &&
                       acy >= 0.0 && acy < atlas_gh {
                        let u = (atlas_gx + acx) / atlas_dim.x;
                        let v = (atlas_gy + acy) / atlas_dim.y;

                        let sample = textureSample(msdf_atlas, msdf_sampler, vec2<f32>(u, v));
                        let sd = msdf_median(sample.r, sample.g, sample.b);

                        let alpha = clamp(px_range * (sd - 0.5) + 0.5, 0.0, 1.0);
                        if alpha > 0.01 {
                            let glyph_color = vec4<f32>(cmd.color.rgb, cmd.color.a * alpha);
                            result = blend_over(result, glyph_color);
                        }
                    }

                    cursor_x += advance_px;
                }
            }
            default: {
            }
        }

        // Shadow
        if ty <= 2u {
            let shadow_alpha = 0.15 * (1.0 - smoothstep(-1.0, 4.0, shadow_d));
            let shadow_color = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
            result = blend_over(result, shadow_color);
        }

        // Animation
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

        // Shape fill (skip for texture and MSDF text — they blend internally)
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
