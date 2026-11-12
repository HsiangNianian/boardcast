//! The ground under the whole page: grey dots in an ordered dither of a soft, slowly drifting field,
//! brighter around the pointer, like a halftone of moving cloud. A fragment shader draws it each frame
//! on a canvas fixed behind the page. Without WebGL the page keeps its still dots from the stylesheet.

use crate::util::{doc, frame, on_passive, reduced, win};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGlProgram, WebGlRenderingContext as Gl, WebGlShader};

const VERTEX: &str = "attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }";

/// The field is value noise in a few octaves, drifting; the pointer adds a soft bump. Each 3 px cell
/// is on or off by an 8x8 Bayer threshold, and an on cell shows a 2 px dot in the ink.
const FRAGMENT: &str = r"
precision mediump float;
uniform vec2 res;
uniform float t;
uniform vec2 ptr;
uniform vec4 ink;
uniform float cell;
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
float fbm(vec2 p) {
  float v = 0.0, a = 0.5;
  for (int k = 0; k < 4; k++) { v += a * noise(p); p = p * 2.03 + vec2(1.7, 9.2); a *= 0.5; }
  return v;
}
float bayer2(vec2 a) { a = floor(a); return fract(dot(a, vec2(0.5, a.y * 0.75))); }
float bayer4(vec2 a) { return bayer2(0.5 * a) * 0.25 + bayer2(a); }
float bayer8(vec2 a) { return bayer4(0.5 * a) * 0.25 + bayer2(a); }
void main() {
  vec2 c = floor(gl_FragCoord.xy / cell);
  vec2 at = c * cell;
  vec2 uv = at / res.y;
  float n = fbm(uv * 2.4 + vec2(t * 0.018, -t * 0.011));
  n = smoothstep(0.38, 0.82, n) * 0.62;
  vec2 d = (at - ptr) / res.y;
  n += 0.5 * exp(-dot(d, d) * 26.0);
  float on = step(bayer8(c), n);
  vec2 f = mod(gl_FragCoord.xy, cell);
  float dotted = step(f.x, cell * 0.67) * step(f.y, cell * 0.67);
  gl_FragColor = vec4(ink.rgb * ink.a, ink.a) * on * dotted;
}
";

fn shader(gl: &Gl, kind: u32, source: &str) -> Option<WebGlShader> {
    let s = gl.create_shader(kind)?;
    gl.shader_source(&s, source);
    gl.compile_shader(&s);
    gl.get_shader_parameter(&s, Gl::COMPILE_STATUS).as_bool().unwrap_or(false).then_some(s)
}

fn program(gl: &Gl) -> Option<WebGlProgram> {
    let p = gl.create_program()?;
    gl.attach_shader(&p, &shader(gl, Gl::VERTEX_SHADER, VERTEX)?);
    gl.attach_shader(&p, &shader(gl, Gl::FRAGMENT_SHADER, FRAGMENT)?);
    gl.link_program(&p);
    gl.get_program_parameter(&p, Gl::LINK_STATUS).as_bool().unwrap_or(false).then_some(p)
}

/// The text colour of the page's theme, as the dots' ink: `#rrggbb` from the `--fg` property.
fn ink() -> [f32; 3] {
    let fg = doc()
        .document_element()
        .and_then(|e| win().get_computed_style(&e).ok().flatten())
        .and_then(|s| s.get_property_value("--fg").ok())
        .unwrap_or_default();
    let hex = fg.trim().trim_start_matches('#');
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).map_or(0.5, |b| f32::from(b) / 255.0);
    if hex.len() >= 6 { [byte(0), byte(2), byte(4)] } else { [0.5, 0.5, 0.5] }
}

pub fn init() {
    let canvas: HtmlCanvasElement = match doc().create_element("canvas").ok().and_then(|e| e.dyn_into().ok()) {
        Some(c) => c,
        None => return,
    };
    canvas.set_class_name("ground");
    let _ = canvas.set_attribute("aria-hidden", "true");
    let Some(gl) = canvas.get_context("webgl").ok().flatten().and_then(|c| c.dyn_into::<Gl>().ok()) else { return };
    let Some(prog) = program(&gl) else { return };
    let Some(body) = doc().body() else { return };
    let _ = body.prepend_with_node_1(&canvas);
    // The stylesheet's still dots give way once the ground draws.
    if let Some(root) = doc().document_element() {
        let _ = root.class_list().add_1("ground-on");
    }
    gl.use_program(Some(&prog));
    let buffer = gl.create_buffer();
    gl.bind_buffer(Gl::ARRAY_BUFFER, buffer.as_ref());
    // One triangle over the whole canvas.
    let corners = js_sys::Float32Array::from(&[-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0][..]);
    gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &corners, Gl::STATIC_DRAW);
    let at = gl.get_attrib_location(&prog, "p") as u32;
    gl.enable_vertex_attrib_array(at);
    gl.vertex_attrib_pointer_with_i32(at, 2, Gl::FLOAT, false, 0, 0);
    gl.enable(Gl::BLEND);
    gl.blend_func(Gl::ONE, Gl::ONE_MINUS_SRC_ALPHA);
    let u = |name: &str| gl.get_uniform_location(&prog, name);
    let (res, time, ptr, ink_at, cell) = (u("res"), u("t"), u("ptr"), u("ink"), u("cell"));

    let pointer = Rc::new(Cell::new((-9999.0f32, -9999.0f32)));
    {
        let pointer = pointer.clone();
        on_passive(&doc(), "pointermove", move |e| {
            if let Some(e) = e.dyn_ref::<web_sys::MouseEvent>() {
                pointer.set((e.client_x() as f32, e.client_y() as f32));
            }
        });
    }
    let still = reduced();
    wasm_bindgen_futures::spawn_local(async move {
        let mut last_ink = 0.0;
        let mut colour = ink();
        let mut drawn_at = -1.0;
        loop {
            let now = frame().await;
            // About thirty frames a second is plenty for something this slow.
            if now - drawn_at < 33.0 {
                continue;
            }
            drawn_at = now;
            if now - last_ink > 1000.0 {
                colour = ink();
                last_ink = now;
            }
            let dpr = win().device_pixel_ratio();
            let (w, h) = (win().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(800.0), win().inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(600.0));
            let (pw, ph) = ((w * dpr).round() as u32, (h * dpr).round() as u32);
            if canvas.width() != pw || canvas.height() != ph {
                canvas.set_width(pw);
                canvas.set_height(ph);
                gl.viewport(0, 0, pw as i32, ph as i32);
            }
            let (px, py) = pointer.get();
            // Light ink on dark paper reads stronger than dark on light, so it is thinner.
            let alpha = if colour.iter().sum::<f32>() > 1.5 { 0.16 } else { 0.13 };
            gl.uniform2f(res.as_ref(), pw as f32, ph as f32);
            gl.uniform1f(time.as_ref(), if still { 0.0 } else { (now / 1000.0) as f32 });
            gl.uniform2f(ptr.as_ref(), px * dpr as f32, ph as f32 - py * dpr as f32);
            gl.uniform4f(ink_at.as_ref(), colour[0], colour[1], colour[2], alpha);
            gl.uniform1f(cell.as_ref(), (3.0 * dpr).round().max(2.0) as f32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(Gl::COLOR_BUFFER_BIT);
            gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        }
    });
}
