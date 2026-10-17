// Main scene shader: instanced meshes, analytic SDF impostors, shadow passes. The renderer
// compiles it with materials.wgsl (procedural materials) in front.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    eye: vec4<f32>,          // xyz camera position; w = 1 perspective, 0 orthographic
    forward: vec4<f32>,      // xyz camera forward
    sun_dir: vec4<f32>,      // xyz direction light travels; w = shadows enabled
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    cut: vec4<f32>,          // xyz focus; w = cut height
    cut2: vec4<f32>,         // x cut radius, y height cut on, z fade radius, w fade on
    params: vec4<f32>,       // x time, y point light count, z shadow texel (world), w pixel footprint:
                             // metres per pixel at 1 m (perspective) or in all (orthographic)
    style: vec4<f32>,        // x flat shadow, y cel bands, z rim, w specular
    viewport: vec4<f32>,     // w, h, 1/w, 1/h
    fog: vec4<f32>,          // rgb colour, w = enabled
    fog2: vec4<f32>,         // x,y centre (xz), z start, w end
    cut3: vec4<f32>,         // x front cut distance, y front cut on
    wind: vec4<f32>,         // xy direction (world x, z), z strength, w gust speed
    push: vec4<f32>,         // xyz the pusher's feet (grass bends away), w radius (0 = none)
    sky_zenith: vec4<f32>,   // rgb straight up; w = 1 when the scene has a sky
    sky_horizon: vec4<f32>,  // rgb at the horizon; w = the sun disc's angular radius (0 = none)
    sky_ground: vec4<f32>,   // rgb well below the horizon; w = stars 0..1
    sky_glow: vec4<f32>,     // rgb the sun's glow; w = cloud cover 0..1
    sky_sun: vec4<f32>,      // xyz toward the sun; w = lightning flash 0..1
    sky_sun_color: vec4<f32>,// rgb the sun disc's radiance (HDR); w = 1 when the moon is up
    sky_moon: vec4<f32>,     // xyz toward the moon
    sky_cloud: vec4<f32>,    // rgb the clouds' colour
    sky_drift: vec4<f32>,    // xy how far the clouds have drifted (m, world x and z)
    levels: vec4<f32>,       // x wetness, y street lights (NIGHT glow), z lit windows
};

struct PointLight {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> point_lights: array<PointLight>;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
@group(0) @binding(4) var point_shadow: texture_depth_2d_array;
@group(0) @binding(5) var<storage, read> point_mats: array<mat4x4<f32>>;

const FLAG_NO_CUT: u32 = 1u;
const FLAG_NO_RECEIVE_SHADOW: u32 = 4u;
const FLAG_CUT_VERTEX: u32 = 8u;
const FLAG_TWO_SIDED: u32 = 16u;
const FLAG_PIXEL: u32 = 32u;
const FLAG_OBJECT: u32 = 64u;
const FLAG_SWAY: u32 = 128u;
const FLAG_GRASS: u32 = 256u;
const FLAG_NIGHT: u32 = 512u;

// How far wind moves a vertex `h` metres above the bottom of its instance. Gusts roll across
// the world as a slow wave along the wind (so a field of grass ripples), with a faster flutter
// on top. Grass bends with the square of its height, much further than foliage, and away from
// the pusher (the player wading through it); a bent blade also sinks so it keeps its length.
fn wind_offset(world: vec3<f32>, h: f32, grass: bool) -> vec3<f32> {
    let w = g.wind;
    let t = g.params.x;
    let dir = vec3<f32>(w.x, 0.0, w.y);
    let along = dot(world.xz, w.xy);
    let gust = 0.6 + 0.4 * sin(along * 0.25 - t * 1.6 * max(w.w, 0.05)) + 0.25 * sin(along * 0.61 - t * 2.9 * max(w.w, 0.05));
    let flutter = sin(t * 4.7 + world.x * 1.31 + world.z * 0.93);
    var k = h * 0.045;
    if (grass) {
        k = h * h * 1.6;
    }
    var off = dir * (w.z * gust * k) + vec3<f32>(-w.y, 0.0, w.x) * (w.z * flutter * k * 0.18);
    if (grass) {
        if (g.push.w > 0.0) {
            let d = world.xz - g.push.xz;
            let dist = length(d);
            if (dist < g.push.w && abs(world.y - g.push.y) < 1.5) {
                let s = 1.0 - dist / g.push.w;
                off += vec3<f32>(d.x, 0.0, d.y) / max(dist, 1e-3) * s * s * h * 1.3;
            }
        }
        let l = length(off.xz);
        off.y -= min(l * l / max(h, 0.05) * 0.5, h * 0.8);
    }
    return off;
}

// Wind on a mesh vertex: `m3` is the instance's position, `ext` its half size in the world.
fn sway_mesh(world: vec3<f32>, flags: u32, m3: vec3<f32>, ext: vec3<f32>) -> vec3<f32> {
    if ((flags & (FLAG_SWAY | FLAG_GRASS)) == 0u) {
        return world;
    }
    let h = max(world.y - (m3.y - ext.y), 0.0);
    return world + wind_offset(world, h, (flags & FLAG_GRASS) != 0u);
}

struct FsOut {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

fn bayer4(frag: vec2<f32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let x = u32(frag.x) % 4u;
    let y = u32(frag.y) % 4u;
    return (m[y * 4u + x] + 0.5) / 16.0;
}

// True when the fragment should be removed by the camera helpers. Meshes handle the height cut
// in the vertex shader (walls are lowered), so they pass `height = false`.
fn cut_away(p: vec3<f32>, frag: vec2<f32>, flags: u32, height: bool) -> bool {
    if ((flags & FLAG_NO_CUT) != 0u) {
        return false;
    }
    if (g.cut3.y > 0.5 && dot(p - g.cut.xyz, g.forward.xyz) < -g.cut3.x) {
        return true;
    }
    if (height && g.cut2.y > 0.5) {
        if (p.y > g.cut.w && length(p.xz - g.cut.xz) < g.cut2.x) {
            return true;
        }
    }
    if (g.cut2.w > 0.5) {
        let f = g.cut.xyz;
        var ro: vec3<f32>;
        var rd: vec3<f32>;
        if (g.eye.w > 0.5) {
            ro = g.eye.xyz;
            rd = normalize(f - ro);
        } else {
            rd = g.forward.xyz;
            ro = f - rd * 1000.0;
        }
        let t_f = dot(f - ro, rd);
        let t_p = dot(p - ro, rd);
        if (t_p < t_f - 0.75) {
            let d = length((p - ro) - rd * t_p);
            let r = g.cut2.z;
            if (d < r) {
                let alpha = smoothstep(0.8, 1.0, d / r);
                if (bayer4(frag) > alpha) {
                    return true;
                }
            }
        }
    }
    return false;
}

fn shadow_factor(p: vec3<f32>, n: vec3<f32>) -> f32 {
    if (g.sun_dir.w < 0.5) {
        return 1.0;
    }
    let texel = g.params.z;
    let lp = g.light_view_proj * vec4<f32>(p + n * texel * 1.5, 1.0);
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || ndc.z > 1.0 || ndc.z < 0.0) {
        return 1.0;
    }
    let ts = 1.0 / f32(textureDimensions(shadow_map).x);
    let d = ndc.z - 0.0008;
    var s = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            s += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(f32(x), f32(y)) * ts, d);
        }
    }
    return s / 9.0;
}

// Shadow of point light shadow slot `slot` at p (1 = lit). Faces: +x -x +y -y +z -z.
fn point_shadow_factor(slot: u32, light: vec3<f32>, p: vec3<f32>, n: vec3<f32>) -> f32 {
    let d = p - light;
    let a = abs(d);
    var face = 0u;
    if (a.x >= a.y && a.x >= a.z) {
        face = select(1u, 0u, d.x > 0.0);
    } else if (a.y >= a.z) {
        face = select(3u, 2u, d.y > 0.0);
    } else {
        face = select(5u, 4u, d.z > 0.0);
    }
    let layer = slot * 6u + face;
    // Pull the sample point toward the light and off the surface (world-space bias).
    let q = p + n * 0.04 - normalize(d) * 0.04;
    let c = point_mats[layer] * vec4<f32>(q, 1.0);
    let ndc = c.xyz / c.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    if (ndc.z > 1.0 || ndc.z < 0.0) {
        return 1.0;
    }
    let ts = 1.0 / f32(textureDimensions(point_shadow).x);
    var s = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let o = (vec2<f32>(f32(x), f32(y)) - 0.5) * ts * 1.5;
            s += textureSampleCompareLevel(point_shadow, shadow_sampler, clamp(uv + o, vec2<f32>(0.0), vec2<f32>(1.0)), layer, ndc.z);
        }
    }
    return s * 0.25;
}

fn view_dir(p: vec3<f32>) -> vec3<f32> {
    if (g.eye.w > 0.5) {
        return normalize(g.eye.xyz - p);
    }
    return -g.forward.xyz;
}

fn quantize(x: f32, bands: f32) -> f32 {
    // Hard-edged bands with a hair of smoothing to avoid shimmering.
    let s = x * bands;
    let f = floor(s);
    return (f + smoothstep(0.45, 0.55, s - f)) / bands;
}

// How wet a surface is in the scene's wet weather (x, 0..1: it darkens and turns glossy) and how
// much of it is standing water (y: level ground pools into puddles, laid by world-space noise
// that spreads as it gets wetter; walls and objects only get damp).
fn wet_amount(p: vec3<f32>, n: vec3<f32>, flags: u32) -> vec2<f32> {
    let w = g.levels.x;
    let up = smoothstep(0.6, 0.92, n.y);
    var pool = 0.0;
    if (up > 0.0 && (flags & FLAG_OBJECT) == 0u) {
        let k = vnoise(vec3<f32>(p.x * 0.31, 0.5, p.z * 0.31), 0x9e7u) * 0.65 + vnoise(vec3<f32>(p.x * 0.97, 2.5, p.z * 0.97), 0x9e8u) * 0.35;
        // A third of level ground pools when soaked, less when merely wet.
        pool = smoothstep(0.62 - 0.08 * w, 0.67 - 0.08 * w, k) * up * w;
    }
    return vec2<f32>(w * mix(0.5, 1.0, up), pool);
}

// What a mirror-like surface reflects toward `r`: the sky's gradient, or the ambient light's
// sky and ground colours when the scene has no sky.
fn sky_reflect(r: vec3<f32>) -> vec3<f32> {
    if (sky_on()) {
        return sky_gradient(r);
    }
    return mix(g.ground_color.rgb, g.sky_color.rgb, clamp(r.y * 0.5 + 0.5, 0.0, 1.0));
}

// `pbr`: a material's roughness and metallic; x < 0 keeps the legacy shading exactly. `refl`:
// how mirror-like the surface is (a material's glass); wet weather adds its own.
fn shade(p: vec3<f32>, n: vec3<f32>, albedo_in: vec3<f32>, emissive: f32, style: u32, flags: u32, pbr: vec2<f32>, refl_in: f32) -> vec3<f32> {
    if (style == 3u) {
        return apply_fog(p, albedo_in * max(1.0, 1.0 + emissive));
    }
    var albedo = albedo_in;
    var refl = refl_in;
    var damp = 0.0;
    if (g.levels.x > 0.0 && style != 0u) {
        let wet = wet_amount(p, n, flags);
        damp = wet.x;
        albedo *= (1.0 - 0.45 * wet.x) * (1.0 - 0.35 * wet.y);
        refl = max(refl, wet.y + 0.12 * wet.x * smoothstep(0.6, 0.92, n.y));
    }
    var sh = 1.0;
    if ((flags & FLAG_NO_RECEIVE_SHADOW) == 0u) {
        sh = shadow_factor(p, n);
    }
    let l = -g.sun_dir.xyz;
    let ndl = dot(n, l);
    let v = view_dir(p);
    let hemi = mix(g.ground_color.rgb, g.sky_color.rgb, n.y * 0.5 + 0.5);
    var col: vec3<f32>;
    if (style == 0u) {
        col = albedo * mix(g.style.x, 1.0, sh);
    } else if (style == 1u) {
        let lit = quantize(clamp(ndl, 0.0, 1.0) * sh, max(g.style.y - 1.0, 1.0));
        let direct = step(0.02, ndl) * sh;
        let amount = select(lit, direct, g.style.y <= 2.0);
        let rim = step(0.72, 1.0 - max(dot(n, v), 0.0)) * g.style.z * (0.4 + 0.6 * amount);
        col = albedo * (hemi + g.sun_color.rgb * amount) + albedo * rim;
        if (pbr.x >= 0.0 && pbr.y > 0.0) {
            // Metals in cel: a hard highlight band, tighter when smoother.
            let hh = normalize(l + v);
            let hl = step(mix(0.995, 0.93, pbr.x), dot(n, hh)) * sh;
            col += g.sun_color.rgb * hl * pbr.y * mix(albedo, vec3<f32>(1.0), 0.5);
        }
    } else {
        let dl = max(ndl, 0.0) * sh;
        let h = normalize(l + v);
        // Wet surfaces shine more.
        let gloss = 1.0 + 3.0 * damp;
        if (pbr.x < 0.0) {
            let spec = pow(max(dot(n, h), 0.0), 40.0 * gloss) * g.style.w * sh * gloss;
            col = albedo * (hemi + g.sun_color.rgb * dl) + g.sun_color.rgb * spec;
        } else {
            let spec = pow(max(dot(n, h), 0.0), mix(80.0, 6.0, pbr.x) * gloss) * g.style.w * sh * (1.2 - pbr.x) * gloss;
            let tint = mix(vec3<f32>(1.0), albedo, pbr.y);
            col = albedo * (1.0 - 0.6 * pbr.y) * (hemi + g.sun_color.rgb * dl) + g.sun_color.rgb * spec * tint;
        }
        if (sky_on()) {
            // Under a sky every lit surface catches a little of it, most at grazing angles
            // (a rough dielectric's sheen): distant roads and walls pale toward the horizon.
            let ndv = clamp(dot(n, v), 0.0, 1.0);
            col += sky_reflect(reflect(-v, n)) * (0.03 + 0.1 * pow(1.0 - ndv, 5.0));
        }
    }
    if (refl > 0.0) {
        // Glass and standing water: the sky by Fresnel's law (a quarter even head-on), and the
        // sun's glint where the reflection meets it.
        let r = reflect(-v, n);
        let f = refl * mix(0.25, 1.0, pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 5.0));
        col = col * (1.0 - f) + sky_reflect(r) * f;
        col += g.sun_color.rgb * pow(max(dot(r, l), 0.0), 600.0) * 5.0 * f * sh;
    }
    let count = u32(g.params.y);
    for (var i = 0u; i < count; i++) {
        let pl = point_lights[i];
        let d = pl.pos_radius.xyz - p;
        let dist = length(d);
        let r = pl.pos_radius.w;
        let ld = d / max(dist, 1e-4);
        if (dist < r) {
            var att = pow(clamp(1.0 - dist / r, 0.0, 1.0), 2.0);
            if (pl.color.w > 0.5 && (flags & FLAG_NO_RECEIVE_SHADOW) == 0u) {
                att *= point_shadow_factor(u32(pl.color.w - 0.5), pl.pos_radius.xyz, p, n);
            }
            var k = max(dot(n, ld), 0.0) * att;
            if (style == 0u) {
                k = att;
            } else if (style == 1u) {
                k = quantize(k, 3.0);
            }
            col += albedo * pl.color.rgb * k;
        }
        // Lamps shine in wet streets and glass well beyond their own pool: a mirror shows the
        // lamp itself.
        if (refl > 0.0 && style != 0u && dist < r * 4.0) {
            let hl = normalize(ld + v);
            let reach = pow(1.0 - dist / (r * 4.0), 2.0);
            col += pl.color.rgb * pow(max(dot(n, hl), 0.0), 96.0) * reach * refl * 2.0;
        }
    }
    col = col + albedo * emissive;
    return apply_fog(p, col);
}

// The colour fog fades into toward `p`: the sky's colour that way when the scene has a sky (so
// fogged geometry melts into it looking up, level or down), else the fog's own.
fn fog_color(p: vec3<f32>) -> vec3<f32> {
    if (sky_on()) {
        return sky_gradient(-view_dir(p));
    }
    return g.fog.rgb;
}

fn apply_fog(p: vec3<f32>, col: vec3<f32>) -> vec3<f32> {
    if (g.fog.w < 0.5) {
        return col;
    }
    let d = length(p.xz - g.fog2.xy);
    return mix(col, fog_color(p), smoothstep(g.fog2.z, g.fog2.w, d));
}

// ---------------------------------------------------------------- sky

fn sky_on() -> bool {
    return g.sky_zenith.w > 0.5;
}

// The sky's colour toward `d` (unit) without its sun disc, moon, stars and clouds: the
// gradient, the sun's glow and a lightning flash. Fog fades into it and wet ground and glass
// reflect it.
fn sky_gradient(d: vec3<f32>) -> vec3<f32> {
    let y = clamp(d.y, -1.0, 1.0);
    var col: vec3<f32>;
    if (y >= 0.0) {
        // The horizon's band thins out quickly upward.
        col = mix(g.sky_zenith.rgb, g.sky_horizon.rgb, exp(-y * 4.5));
    } else {
        // Below it, distance haze keeps the horizon's colour a while before the ground's.
        let t = smoothstep(0.0, 0.6, -y);
        col = mix(g.sky_horizon.rgb, g.sky_ground.rgb, t * t);
    }
    // The sun's glow: broad and strongest along the horizon, plus a tight halo. It lingers on
    // the horizon after sunset.
    let mu = max(dot(d, g.sky_sun.xyz), 0.0);
    let band = exp(-abs(y) * 3.0);
    col += g.sky_glow.rgb * (pow(mu, 6.0) * (0.08 + 0.27 * band) + pow(mu, 64.0) * 0.3);
    col += vec3<f32>(0.62, 0.68, 0.85) * (g.sky_sun.w * 0.55);
    return col;
}

// The angle (radians) between unit vectors, accurate for small angles.
fn angle_between(a: vec3<f32>, b: vec3<f32>) -> f32 {
    return 2.0 * asin(clamp(length(a - b) * 0.5, 0.0, 1.0));
}

// Stars: one candidate per cell of a cube-sphere grid (about 0.6 degrees), a soft dot about a
// pixel wide (`px`: radians per pixel), a few bright ones, twinkling.
fn sky_stars(d: vec3<f32>, px: f32) -> vec3<f32> {
    let a = abs(d);
    var uv: vec2<f32>;
    var face: i32;
    if (a.x >= a.y && a.x >= a.z) {
        uv = d.yz / a.x;
        face = select(1, 0, d.x > 0.0);
    } else if (a.y >= a.z) {
        uv = d.xz / a.y;
        face = select(3, 2, d.y > 0.0);
    } else {
        uv = d.xy / a.z;
        face = select(5, 4, d.z > 0.0);
    }
    let n = 90.0;
    let q = uv * n;
    let cell = floor(q);
    let h = hash3i(i32(cell.x), i32(cell.y), face, 0x51a2u);
    if (rnd(h) > 0.055) {
        return vec3<f32>(0.0);
    }
    let h2 = pcg(h);
    let at = cell + vec2<f32>(0.2) + 0.6 * vec2<f32>(rnd(h2), rnd_lo(h2));
    // A cell spans about 1/n radians at the face's centre, less toward its edges.
    let dist = length(q - at) / (n * (1.0 + dot(uv, uv)));
    let r = max(px * 0.8, 1e-5);
    let bright = 0.3 + 3.5 * pow(rnd(pcg(h2)), 7.0);
    let twinkle = 0.8 + 0.2 * sin(g.params.x * (1.7 + 2.3 * rnd_lo(h)) + rnd(h2) * 6.2832);
    let tint = mix(vec3<f32>(0.75, 0.82, 1.0), vec3<f32>(1.0, 0.88, 0.72), rnd_lo(h2));
    return tint * (bright * twinkle * exp(-dist * dist / (r * r)));
}

// The moon: a disc with darker seas and a soft halo. rgb added, a = how much of the pixel the
// disc covers (it hides the stars behind it).
fn sky_moon(d: vec3<f32>, px: f32) -> vec4<f32> {
    let m = g.sky_moon.xyz;
    let r = 0.0125;
    let ang = angle_between(d, m);
    let disc = 1.0 - smoothstep(r - px, r + px, ang);
    let side = normalize(cross(m, vec3<f32>(1e-4, 1.0, 0.0)));
    let up = cross(side, m);
    let lp = vec2<f32>(dot(d - m, side), dot(d - m, up)) / r;
    let seas = vnoise(vec3<f32>(lp * 2.2 + 3.7, 1.3), 0x6d6fu) * 0.6 + vnoise(vec3<f32>(lp * 5.1, 7.9), 0x6d70u) * 0.4;
    let face = vec3<f32>(0.92, 0.94, 1.0) * (1.0 - 0.3 * smoothstep(0.45, 0.62, seas));
    let halo = vec3<f32>(0.55, 0.62, 0.8) * (exp(-ang / 0.02) * 0.35 + exp(-ang / 0.12) * 0.06);
    return vec4<f32>(face * (2.4 * disc) + halo * (1.0 - disc), disc);
}

// Clouds on a layer 1.4 km up, drifting with the wind: soft, warped fractal noise thinned by
// the cover, darker where thick, edges lit toward the sun, fading into the horizon. rgb, a =
// coverage.
fn sky_clouds(d: vec3<f32>, px: f32) -> vec4<f32> {
    let cover = g.sky_glow.w;
    if (cover <= 0.001 || d.y <= 0.015) {
        return vec4<f32>(0.0);
    }
    let t = max(1400.0 - g.eye.y, 200.0) / d.y;
    let scale = 1.0 / 2200.0;
    let p = (g.eye.xz + d.xz * t + g.sky_drift.xy) * scale;
    // The footprint in noise units: grazing rays stretch a pixel over far more cloud.
    let fw = px * t * scale / max(d.y, 0.08);
    let q = vec3<f32>(p, g.params.x * 0.004);
    let warp = vec2<f32>(fbm(q + vec3<f32>(5.2, 1.3, 0.0), 3u, 1.0, fw, 0xc11u), fbm(q + vec3<f32>(1.7, 9.2, 4.1), 3u, 1.0, fw, 0xc12u)) - 0.5;
    let v = fbm(q + vec3<f32>(warp * 0.9, 0.0), 5u, 1.0, fw, 0xc13u);
    let th = mix(0.67, 0.32, cover);
    let a = smoothstep(th - 0.08, th + 0.1, v) * smoothstep(0.015, 0.16, d.y);
    let thick = smoothstep(th, th + 0.28, v);
    let mu = max(dot(d, g.sky_sun.xyz), 0.0);
    var col = g.sky_cloud.rgb * mix(1.0, 0.62, thick * (0.35 + 0.65 * cover));
    col += g.sky_glow.rgb * (pow(mu, 8.0) * (1.0 - thick) * 0.8);
    // Lightning lights the clouds from inside, the thick ones most.
    col *= 1.0 + g.sky_sun.w * (0.6 + 0.8 * thick);
    return vec4<f32>(col, a);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// A full-screen triangle at the far plane: drawn after the opaque geometry, it fills only what
// that left.
@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> SkyOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    let ndc = uv * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(ndc, 1.0, 1.0);
    o.ndc = ndc;
    return o;
}

@fragment
fn fs_sky(in: SkyOut) -> FsOut {
    var d = g.forward.xyz;
    if (g.eye.w > 0.5) {
        let far = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
        d = normalize(far.xyz / far.w - g.eye.xyz);
    }
    let px = g.params.w;
    var col = sky_gradient(d);
    let above = smoothstep(-0.004, 0.006, d.y);
    let r = g.sky_horizon.w;
    if (r > 0.0) {
        // The sun's disc, darker toward its rim, hidden below the horizon.
        let k = clamp(angle_between(d, g.sky_sun.xyz) / r, 0.0, 2.0);
        let disc = 1.0 - smoothstep(1.0 - px / r, 1.0 + px / r, k);
        col += g.sky_sun_color.rgb * (disc * (1.0 - 0.4 * min(k * k, 1.0)) * above);
    }
    var hidden = 0.0;
    if (g.sky_sun_color.w > 0.5) {
        let moon = sky_moon(d, px);
        col += moon.rgb * above;
        hidden = moon.a;
    }
    if (g.sky_ground.w > 0.0) {
        col += sky_stars(d, px) * (g.sky_ground.w * smoothstep(0.02, 0.25, d.y) * (1.0 - hidden));
    }
    let clouds = sky_clouds(d, px);
    col = mix(col, clouds.rgb, clouds.a);
    var o: FsOut;
    o.color = vec4<f32>(col, 1.0);
    o.normal = vec4<f32>(0.0);
    return o;
}

// The normal buffer's w is the outline group (folded into 1..1023, 0 = background) plus 1024
// on characters and objects, negative for pixel-art objects: the composite reads the sign as
// the pixel-art mask and the size as the part of the scene (exact in 16-bit floats).
fn group_out(n: vec3<f32>, group: u32, flags: u32) -> vec4<f32> {
    var gi = select(0u, (group - 1u) % 1023u + 1u, group != 0u);
    if ((flags & FLAG_OBJECT) != 0u) {
        gi = gi + 1024u;
    }
    let g = f32(gi);
    return vec4<f32>(n, select(g, -g, (flags & FLAG_PIXEL) != 0u));
}

// ---------------------------------------------------------------- meshes

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(9) color: vec4<f32>,
};

struct InstIn {
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
    @location(7) color: vec4<f32>,
    @location(8) params: vec4<u32>,   // style, flags, group, material (slot | seed << 12)
    @location(10) ext: vec4<f32>,     // xyz half extents for materials (0 = from the columns)
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) params: vec4<u32>,
    // Materials: the part-local metric position and normal (before sway) and half extents.
    @location(4) local: vec3<f32>,
    @location(5) lnormal: vec3<f32>,
    @location(6) @interpolate(flat) ext: vec3<f32>,
};

@vertex
fn vs_mesh(v: MeshIn, i: InstIn) -> MeshOut {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    let wp = model * vec4<f32>(v.pos, 1.0);
    let c0 = i.m0.xyz;
    let c1 = i.m1.xyz;
    let c2 = i.m2.xyz;
    let cof = mat3x3<f32>(cross(c1, c2), cross(c2, c0), cross(c0, c1));
    var n = cof * v.normal;
    if (dot(cross(c0, c1), c2) < 0.0) {
        n = -n;
    }
    var o: MeshOut;
    var world = sway_mesh(wp.xyz, i.params.y, i.m3.xyz, 0.5 * (abs(c0) + abs(c1) + abs(c2)));
    if (g.cut2.y > 0.5 && (i.params.y & FLAG_CUT_VERTEX) != 0u) {
        // Large meshes (terrain): lower each vertex near the player instead of the whole mesh.
        if (length(world.xz - g.cut.xz) < g.cut2.x) {
            world.y = min(world.y, g.cut.w);
        }
    } else if (g.cut2.y > 0.5 && (i.params.y & FLAG_NO_CUT) == 0u) {
        // "Walls down": instances whose footprint touches the cut circle are lowered to the
        // cut height; instances entirely above it are hidden.
        let ext = 0.5 * (abs(c0) + abs(c1) + abs(c2));
        let c = i.m3.xyz;
        let d = max(abs(g.cut.xz - c.xz) - ext.xz, vec2<f32>(0.0));
        if (length(d) < g.cut2.x) {
            if (c.y - ext.y >= g.cut.w - 0.01) {
                o.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
                return o;
            }
            world.y = min(world.y, g.cut.w);
        }
    }
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.world = world;
    o.normal = normalize(n);
    o.color = vec4<f32>(i.color.rgb * v.color.rgb, i.color.a);
    o.params = i.params;
    if ((i.params.w & 0xfffu) != 0u) {
        // The part-local metric frame (unit axes along the columns), for materials.
        let ax = vec3<f32>(length(c0), length(c1), length(c2));
        let rel = wp.xyz - i.m3.xyz;
        o.local = vec3<f32>(dot(rel, c0) / max(ax.x, 1e-6), dot(rel, c1) / max(ax.y, 1e-6), dot(rel, c2) / max(ax.z, 1e-6));
        o.lnormal = v.normal / max(ax, vec3<f32>(1e-6));
        o.ext = select(0.5 * ax, i.ext.xyz, any(i.ext.xyz != vec3<f32>(0.0)));
    }
    return o;
}

@fragment
fn fs_mesh(in: MeshOut, @builtin(front_facing) front: bool) -> FsOut {
    if (cut_away(in.world, in.clip.xy, in.params.y, false)) {
        discard;
    }
    var o: FsOut;
    var albedo = in.color.rgb;
    var emis = in.color.a;
    var pbr = vec2<f32>(-1.0, 0.0);
    var refl = 0.0;
    let mid = in.params.w & 0xfffu;
    if (mid != 0u && (front || (in.params.y & FLAG_TWO_SIDED) != 0u)) {
        let side = select(-1.0, 1.0, front);
        let mo = eval_material(mid, in.params.w >> 12u, albedo, in.local, side * normalize(in.lnormal), in.world, side * normalize(in.normal), in.ext);
        albedo = mo.albedo;
        emis += mo.glow;
        refl = mo.refl;
        pbr = materials[mid].pbr.xy;
    }
    if ((in.params.y & FLAG_NIGHT) != 0u) {
        emis *= g.levels.y;
    }
    if (!front && (in.params.y & FLAG_TWO_SIDED) != 0u) {
        let nb = -normalize(in.normal);
        o.color = vec4<f32>(shade(in.world, nb, albedo * 0.85, emis, in.params.x, in.params.y, pbr, refl), 1.0);
        o.normal = group_out(nb, in.params.z, in.params.y);
        return o;
    }
    if (!front) {
        // Inside of a cut-open solid: draw a flat "cross-section" cap.
        o.color = vec4<f32>(in.color.rgb * 0.42, 1.0);
        o.normal = group_out(vec3<f32>(0.0, 1.0, 0.0), in.params.z, in.params.y);
        return o;
    }
    let n = normalize(in.normal);
    o.color = vec4<f32>(shade(in.world, n, albedo, emis, in.params.x, in.params.y, pbr, refl), 1.0);
    // The outline buffer always gets the geometric normal.
    o.normal = group_out(n, in.params.z, in.params.y);
    return o;
}

@vertex
fn vs_mesh_shadow(v: MeshIn, i: InstIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    let ext = 0.5 * (abs(i.m0.xyz) + abs(i.m1.xyz) + abs(i.m2.xyz));
    let world = sway_mesh((model * vec4<f32>(v.pos, 1.0)).xyz, i.params.y, i.m3.xyz, ext);
    return g.view_proj * vec4<f32>(world, 1.0);
}

// ---------------------------------------------------------------- SDF impostors

struct SdfIn {
    @location(0) a: vec4<f32>,      // xyz, radius a
    @location(1) b: vec4<f32>,      // xyz, radius b
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<u32>,
};

struct SdfOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, sample) world: vec3<f32>,
    @location(1) @interpolate(flat) a: vec4<f32>,
    @location(2) @interpolate(flat) b: vec4<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) params: vec4<u32>,
};

struct SdfFsOut {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

fn cube_corner(vi: u32) -> vec3<f32> {
    var idx = array<u32, 36>(
        5u, 1u, 3u, 5u, 3u, 7u,
        0u, 4u, 6u, 0u, 6u, 2u,
        6u, 7u, 3u, 6u, 3u, 2u,
        0u, 1u, 5u, 0u, 5u, 4u,
        4u, 5u, 7u, 4u, 7u, 6u,
        1u, 0u, 2u, 1u, 2u, 3u);
    let c = idx[vi];
    return vec3<f32>(select(-1.0, 1.0, (c & 1u) != 0u), select(-1.0, 1.0, (c & 2u) != 0u), select(-1.0, 1.0, (c & 4u) != 0u));
}

@vertex
fn vs_sdf(@builtin(vertex_index) vi: u32, s: SdfIn) -> SdfOut {
    let c = cube_corner(vi);
    var a = s.a.xyz;
    var b = s.b.xyz;
    if ((s.params.y & (FLAG_SWAY | FLAG_GRASS)) != 0u) {
        // Analytic shapes move whole, as if their centre stood 2.5 m above a trunk's root.
        let off = wind_offset((a + b) * 0.5, 2.5 + max(s.a.w, s.b.w), false);
        a += off;
        b += off;
    }
    let r = max(s.a.w, s.b.w) * 1.02 + 0.001;
    let axis = b - a;
    let len = length(axis);
    var dir = vec3<f32>(0.0, 1.0, 0.0);
    if (len > 1e-5) {
        dir = axis / len;
    }
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(dir.y) > 0.9);
    let t1 = normalize(cross(dir, up));
    let t2 = cross(t1, dir);
    let mid = (a + b) * 0.5;
    let w = mid + dir * (c.y * (len * 0.5 + r)) + t1 * (c.x * r) + t2 * (c.z * r);
    var o: SdfOut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    o.a = vec4<f32>(a, s.a.w);
    o.b = vec4<f32>(b, s.b.w);
    o.color = s.color;
    o.params = s.params;
    return o;
}

// Ray / rounded-cone intersection (Inigo Quilez, MIT). Returns (t, normal) or t < 0 on miss.
fn round_cone(ro: vec3<f32>, rd: vec3<f32>, pa: vec3<f32>, pb: vec3<f32>, ra: f32, rb: f32) -> vec4<f32> {
    let ba = pb - pa;
    let oa = ro - pa;
    let ob = ro - pb;
    let rr = ra - rb;
    let m0 = dot(ba, ba);
    let m1 = dot(ba, oa);
    let m2 = dot(ba, rd);
    let m3 = dot(rd, oa);
    let m5 = dot(oa, oa);
    let m6 = dot(ob, rd);
    let m7 = dot(ob, ob);
    let d2 = m0 - rr * rr;
    if (d2 > 1e-8) {
        let k2 = d2 - m2 * m2;
        let k1 = d2 * m3 - m1 * m2 + m2 * rr * ra;
        let k0 = d2 * m5 - m1 * m1 + m1 * rr * ra * 2.0 - m0 * ra * ra;
        let h = k1 * k1 - k0 * k2;
        if (h < 0.0) {
            return vec4<f32>(-1.0);
        }
        if (abs(k2) > 1e-8) {
            let t = (-sqrt(h) - k1) / k2;
            let y = m1 - ra * rr + t * m2;
            if (y > 0.0 && y < d2) {
                return vec4<f32>(t, normalize(d2 * (oa + t * rd) - ba * y));
            }
        }
    }
    let h1 = m3 * m3 - m5 + ra * ra;
    let h2 = m6 * m6 - m7 + rb * rb;
    if (max(h1, h2) < 0.0) {
        return vec4<f32>(-1.0);
    }
    var r = vec4<f32>(1e20);
    if (h1 > 0.0) {
        let t = -m3 - sqrt(h1);
        r = vec4<f32>(t, (oa + t * rd) / ra);
    }
    if (h2 > 0.0) {
        let t = -m6 - sqrt(h2);
        if (t < r.x) {
            r = vec4<f32>(t, (ob + t * rd) / rb);
        }
    }
    if (r.x > 1e19) {
        return vec4<f32>(-1.0);
    }
    return r;
}

fn sdf_ray(world: vec3<f32>, a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    var ro: vec3<f32>;
    var rd: vec3<f32>;
    if (g.eye.w > 0.5) {
        ro = g.eye.xyz;
        rd = normalize(world - ro);
    } else {
        rd = g.forward.xyz;
        let span = length(b.xyz - a.xyz) + 4.0 * max(a.w, b.w) + 1.0;
        ro = world - rd * span * 2.0;
    }
    let hit = round_cone(ro, rd, a.xyz, b.xyz, a.w, b.w);
    if (hit.x < 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    return vec4<f32>(ro + rd * hit.x, 1.0);
}

@fragment
fn fs_sdf(in: SdfOut) -> SdfFsOut {
    var ro: vec3<f32>;
    var rd: vec3<f32>;
    if (g.eye.w > 0.5) {
        ro = g.eye.xyz;
        rd = normalize(in.world - ro);
    } else {
        rd = g.forward.xyz;
        let span = length(in.b.xyz - in.a.xyz) + 4.0 * max(in.a.w, in.b.w) + 1.0;
        ro = in.world - rd * span * 2.0;
    }
    let hit = round_cone(ro, rd, in.a.xyz, in.b.xyz, in.a.w, in.b.w);
    if (hit.x < 0.0) {
        discard;
    }
    let p = ro + rd * hit.x;
    if (cut_away(p, in.clip.xy, in.params.y, true)) {
        discard;
    }
    let n = normalize(hit.yzw);
    let clip = g.view_proj * vec4<f32>(p, 1.0);
    var o: SdfFsOut;
    var albedo = in.color.rgb;
    var emis = in.color.a;
    var pbr = vec2<f32>(-1.0, 0.0);
    var refl = 0.0;
    let mid = in.params.w & 0xfffu;
    if (mid != 0u) {
        // The local frame runs along a->b, built as vs_sdf builds it; y is the axis.
        let axis = in.b.xyz - in.a.xyz;
        let len = length(axis);
        var dir = vec3<f32>(0.0, 1.0, 0.0);
        if (len > 1e-5) {
            dir = axis / len;
        }
        let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(dir.y) > 0.9);
        let t1 = normalize(cross(dir, up));
        let t2 = cross(t1, dir);
        let rel = p - (in.a.xyz + in.b.xyz) * 0.5;
        let lp = vec3<f32>(dot(rel, t1), dot(rel, dir), dot(rel, t2));
        let ln = vec3<f32>(dot(n, t1), dot(n, dir), dot(n, t2));
        let r = max(in.a.w, in.b.w);
        let mo = eval_material(mid, in.params.w >> 12u, albedo, lp, ln, p, n, vec3<f32>(r, len * 0.5 + r, r));
        albedo = mo.albedo;
        emis += mo.glow;
        refl = mo.refl;
        pbr = materials[mid].pbr.xy;
    }
    if ((in.params.y & FLAG_NIGHT) != 0u) {
        emis *= g.levels.y;
    }
    o.color = vec4<f32>(shade(p, n, albedo, emis, in.params.x, in.params.y, pbr, refl), 1.0);
    o.normal = group_out(n, in.params.z, in.params.y);
    o.depth = clip.z / clip.w;
    return o;
}

@fragment
fn fs_sdf_shadow(in: SdfOut) -> @builtin(frag_depth) f32 {
    let h = sdf_ray(in.world, in.a, in.b);
    if (h.w < 0.0) {
        discard;
    }
    let clip = g.view_proj * vec4<f32>(h.xyz, 1.0);
    return clip.z / clip.w;
}

// ---------------------------------------------------------------- text (SDF glyphs)

@group(1) @binding(0) var font_tex: texture_2d<f32>;
@group(1) @binding(1) var font_samp: sampler;

struct GlyphIn {
    @location(0) corner: vec4<f32>,   // xyz bottom-left corner; w = plane normal x
    @location(1) ax: vec4<f32>,       // xyz glyph width axis; w = normal y
    @location(2) ay: vec4<f32>,       // xyz glyph height axis; w = normal z
    @location(3) uv: vec4<f32>,       // atlas rect (u0, v0 top, u1, v1 bottom)
    @location(4) color: vec4<f32>,    // rgb; w = weight
    @location(5) params: vec4<u32>,   // flags, group
};

struct GlyphOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) normal: vec3<f32>,
    @location(4) @interpolate(flat) params: vec4<u32>,
};

@vertex
fn vs_text(@builtin(vertex_index) vi: u32, gl: GlyphIn) -> GlyphOut {
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let q = quad[vi];
    let w = gl.corner.xyz + gl.ax.xyz * q.x + gl.ay.xyz * q.y;
    var o: GlyphOut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    o.uv = vec2<f32>(mix(gl.uv.x, gl.uv.z, q.x), mix(gl.uv.w, gl.uv.y, q.y));
    o.color = gl.color;
    o.normal = vec3<f32>(gl.corner.w, gl.ax.w, gl.ay.w);
    o.params = gl.params;
    return o;
}

@fragment
fn fs_text(in: GlyphOut) -> FsOut {
    let sd = textureSample(font_tex, font_samp, in.uv).r;
    let aa = max(fwidth(sd), 1e-4) * 0.7;
    let edge = 0.5 - in.color.w;
    let a = smoothstep(edge - aa, edge + aa, sd);
    if (a < 0.01 || cut_away(in.world, in.clip.xy, in.params.x, true)) {
        discard;
    }
    var o: FsOut;
    o.color = vec4<f32>(apply_fog(in.world, in.color.rgb), a);
    o.normal = group_out(normalize(in.normal), in.params.y, 0u);
    return o;
}
