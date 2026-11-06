//! Lighting: a screen-sized light map (dark ambient + additive light sprites) multiplied over the
//! scene, plus a little additive bloom on the brightest sources. Also generates the procedural
//! textures (light falloff, concrete, earth) used by the renderer.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;

const VERTEX: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}
";

const FRAGMENT: &str = "#version 100
precision lowp float;

varying vec4 color;
varying vec2 uv;

uniform sampler2D Texture;

void main() {
    gl_FragColor = texture2D(Texture, uv) * color;
}
";

/// One light: world-pixel position, radius, colour (alpha = intensity).
#[derive(Clone, Copy)]
pub struct Light {
    pub pos: Vec2,
    pub radius: f32,
    pub color: Color,
    /// Also bloom on top of the scene (for bright sources: flames, bulbs, flashes).
    pub bloom: bool,
}

/// A downward cone of light from a ceiling lamp.
#[derive(Clone, Copy)]
pub struct Cone {
    pub apex: Vec2,
    pub height: f32,
    /// Width at the bottom.
    pub width: f32,
    pub color: Color,
}

pub struct Lighting {
    glow: Texture2D,
    cone: Texture2D,
    pub cones: Vec<Cone>,
    additive: Material,
    multiply: Material,
    target: Option<(RenderTarget, u32, u32)>,
    pub lights: Vec<Light>,
    /// World rects that ignore lighting (e.g. the daylit surface).
    pub fullbright: Vec<Rect>,
    pub concrete: Texture2D,
    pub earth: Texture2D,
}

fn material(color_blend: BlendState) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend: Some(color_blend),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Zero,
                    BlendFactor::One,
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("lighting shader")
}

/// Small deterministic hash noise in 0..1.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Tileable value noise (period `p` cells).
fn value_noise(x: f32, y: f32, p: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |v: f32| v * v * (3.0 - 2.0 * v);
    let h = |a: i32, b: i32| hash(a.rem_euclid(p), b.rem_euclid(p), seed);
    let top = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * s(fx);
    let bottom = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * s(fx);
    top + (bottom - top) * s(fy)
}

fn fbm(x: f32, y: f32, size: f32, seed: u32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let mut cells = 4;
    for o in 0..4 {
        v += amp
            * value_noise(
                x / size * cells as f32,
                y / size * cells as f32,
                cells,
                seed + o,
            );
        amp *= 0.5;
        cells *= 2;
    }
    v
}

fn texture(img: &Image) -> Texture2D {
    let t = Texture2D::from_image(img);
    t.set_filter(FilterMode::Linear);
    // Let draw calls tile the texture by asking for a source rect larger than it.
    unsafe {
        let gl = get_internal_gl();
        gl.quad_context.texture_set_wrap(
            t.raw_miniquad_id(),
            macroquad::miniquad::TextureWrap::Repeat,
            macroquad::miniquad::TextureWrap::Repeat,
        );
    }
    t
}

impl Lighting {
    pub fn new() -> Self {
        // Soft radial falloff.
        let n = 128u16;
        let mut img = Image::gen_image_color(n, n, BLANK);
        for y in 0..n {
            for x in 0..n {
                let d = vec2(x as f32 + 0.5, y as f32 + 0.5).distance(vec2(64.0, 64.0)) / 64.0;
                let a = (1.0 - d).clamp(0.0, 1.0);
                let a = a * a * (3.0 - 2.0 * a);
                img.set_pixel(x as u32, y as u32, Color::new(1.0, 1.0, 1.0, a));
            }
        }
        let glow = Texture2D::from_image(&img);
        glow.set_filter(FilterMode::Linear);

        // Lamp cone: narrow and bright at the top, widening and fading toward the floor,
        // with soft edges.
        let mut img = Image::gen_image_color(n, n, BLANK);
        for y in 0..n {
            for x in 0..n {
                let (u, v) = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
                let hw = 0.06 + 0.44 * v;
                let e = ((u - 0.5).abs() / hw).min(1.0);
                let edge = 1.0 - e * e;
                let fall = 1.0 - 0.6 * v;
                img.set_pixel(
                    x as u32,
                    y as u32,
                    Color::new(1.0, 1.0, 1.0, (edge * fall).max(0.0)),
                );
            }
        }
        let cone = Texture2D::from_image(&img);
        cone.set_filter(FilterMode::Linear);

        // Poured concrete: blotchy fbm, board-form lines, rust streaks running down.
        let size = 256u16;
        let mut img = Image::gen_image_color(size, size, BLANK);
        for y in 0..size {
            for x in 0..size {
                let (fx, fy) = (x as f32, y as f32);
                let mut v = 0.78 + 0.28 * (fbm(fx, fy, size as f32, 7) - 0.5);
                v += 0.06 * (hash(x as i32, y as i32, 3) - 0.5); // grit
                if y % 64 == 0 {
                    v -= 0.07; // form-board seams
                }
                let streak = value_noise(fx / 9.0, fy / 90.0, 28, 11);
                let rust = ((streak - 0.62) * 3.0).clamp(0.0, 1.0);
                let c = Color::new(v - 0.02 * rust, v - 0.06 * rust, v - 0.1 * rust, 1.0);
                img.set_pixel(x as u32, y as u32, c);
            }
        }
        let concrete = texture(&img);

        // Packed earth with pebbles.
        let mut img = Image::gen_image_color(size, size, BLANK);
        for y in 0..size {
            for x in 0..size {
                let (fx, fy) = (x as f32, y as f32);
                let v = 0.75 + 0.35 * (fbm(fx, fy, size as f32, 21) - 0.5);
                let pebble = value_noise(fx / 6.0, fy / 6.0, 42, 5);
                let v = if pebble > 0.8 { v + 0.18 } else { v };
                let g = 0.06 * (hash(x as i32, y as i32, 9) - 0.5);
                img.set_pixel(
                    x as u32,
                    y as u32,
                    Color::new(v + g, v * 0.86 + g, v * 0.72 + g, 1.0),
                );
            }
        }
        let earth = texture(&img);

        Self {
            glow,
            cone,
            cones: Vec::with_capacity(128),
            additive: material(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::One,
            )),
            multiply: material(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::DestinationColor),
                BlendFactor::Zero,
            )),
            target: None,
            lights: Vec::with_capacity(512),
            fullbright: Vec::new(),
            concrete,
            earth,
        }
    }

    pub fn add(&mut self, pos: Vec2, radius: f32, color: Color) {
        self.lights.push(Light {
            pos,
            radius,
            color,
            bloom: false,
        });
    }

    pub fn add_bloom(&mut self, pos: Vec2, radius: f32, color: Color) {
        self.lights.push(Light {
            pos,
            radius,
            color,
            bloom: true,
        });
    }

    pub fn add_cone(&mut self, apex: Vec2, height: f32, width: f32, color: Color) {
        self.cones.push(Cone {
            apex,
            height,
            width,
            color,
        });
    }

    fn cone_sprite(&self, c: &Cone, k: f32) {
        draw_texture_ex(
            &self.cone,
            c.apex.x - c.width / 2.0,
            c.apex.y,
            Color::new(c.color.r, c.color.g, c.color.b, c.color.a * k),
            DrawTextureParams {
                dest_size: Some(vec2(c.width, c.height)),
                ..Default::default()
            },
        );
    }

    fn sprite(&self, l: &Light, k: f32) {
        let r = l.radius;
        draw_texture_ex(
            &self.glow,
            l.pos.x - r,
            l.pos.y - r,
            Color::new(l.color.r, l.color.g, l.color.b, l.color.a * k),
            DrawTextureParams {
                dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                ..Default::default()
            },
        );
    }

    /// Light the scene drawn so far. `world` is the camera the scene was drawn with; `screen`
    /// is the letterboxed UI camera; `view` is the visible logical rect (incl. letterbox).
    /// Lights only affect `field` (logical screen rect); the light list is cleared.
    pub fn apply(
        &mut self,
        world: &Camera2D,
        screen: &Camera2D,
        view: Rect,
        field: Rect,
        ambient: Color,
    ) {
        let (w, h) = (
            screen_width().max(1.0) as u32,
            screen_height().max(1.0) as u32,
        );
        // Half resolution is plenty for soft light and saves fill rate on phones.
        let (tw, th) = ((w / 2).max(1), (h / 2).max(1));
        if self.target.as_ref().is_none_or(|t| (t.1, t.2) != (tw, th)) {
            let rt = render_target(tw, th);
            rt.texture.set_filter(FilterMode::Linear);
            self.target = Some((rt, tw, th));
        }
        let rt = self.target.as_ref().unwrap().0.clone();

        // 1. Light map: ambient, plus every light added on top.
        set_camera(&Camera2D {
            target: world.target,
            zoom: vec2(world.zoom.x, world.zoom.y),
            render_target: Some(rt.clone()),
            ..Default::default()
        });
        clear_background(ambient);
        for r in &self.fullbright {
            draw_rectangle(r.x, r.y, r.w, r.h, WHITE);
        }
        gl_use_material(&self.additive);
        for l in &self.lights {
            self.sprite(l, 1.0);
        }
        for c in &self.cones {
            self.cone_sprite(c, 1.0);
        }
        gl_use_default_material();

        // 2. Multiply the light map over the field.
        set_camera(screen);
        // SILO_DEBUG_LIGHT=1 shows the raw light map instead of applying it.
        if std::env::var_os("SILO_DEBUG_LIGHT").is_none() {
            gl_use_material(&self.multiply);
        }
        // Only the field rect: map it to the matching part of the light map.
        let u0 = (field.x - view.x) / view.w;
        let v0 = (field.y - view.y) / view.h;
        let src = Rect::new(
            u0 * tw as f32,
            v0 * th as f32,
            field.w / view.w * tw as f32,
            field.h / view.h * th as f32,
        );
        draw_texture_ex(
            &rt.texture,
            field.x,
            field.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(field.w, field.h)),
                source: Some(src),
                flip_y: false,
                ..Default::default()
            },
        );
        gl_use_default_material();

        // 3. Bloom: bright sources glow on top of the lit scene.
        set_camera(world);
        gl_use_material(&self.additive);
        for l in self.lights.iter().filter(|l| l.bloom) {
            self.sprite(l, 0.18);
        }
        // A faint visible beam in the dusty air under each lamp.
        for c in &self.cones {
            self.cone_sprite(c, 0.07);
        }
        gl_use_default_material();
        self.lights.clear();
        self.cones.clear();
        self.fullbright.clear();
    }
}
