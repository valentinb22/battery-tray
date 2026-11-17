//! Vista previa: renderiza los iconos a varios tamaños y los guarda ampliados en un BMP.
//! Uso: cargo run --release --example icons -- salida.bmp
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;
#[path = "../src/icon.rs"]
mod icon;

use icon::Glyph;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "icons.bmp".into());
    let sizes = [16, 20, 24, 32];
    let cases: [(Glyph, Option<u8>, bool, bool); 6] = [
        (Glyph::Keyboard, Some(100), false, false),
        (Glyph::Headset, Some(64), true, true),
        (Glyph::Headset, Some(64), false, true),
        (Glyph::Mouse, Some(12), false, false),
        (Glyph::Battery, Some(45), false, false),
        (Glyph::Battery, None, false, false),
    ];
    let zoom = 6usize;
    let cell = 32 * zoom + 12;
    // Fila superior: tema oscuro; fila inferior: tema claro.
    let width = cell * cases.len();
    let height = cell * sizes.len() * 2;
    let mut img = vec![0u32; width * height];
    for (ti, light) in [false, true].into_iter().enumerate() {
        let bg = if light { 0xFFF3F3F3 } else { 0xFF202020 };
        for (si, &size) in sizes.iter().enumerate() {
            for (ci, &(g, lvl, bolt, chg)) in cases.iter().enumerate() {
                let (s, px) = icon::pixels(size, g, lvl, bolt, chg, light);
                let ox = ci * cell;
                let oy = (ti * sizes.len() + si) * cell;
                for y in 0..cell {
                    for x in 0..cell {
                        img[(oy + y) * width + ox + x] = bg;
                    }
                }
                for y in 0..s * zoom {
                    for x in 0..s * zoom {
                        let c = px[(y / zoom) * s + x / zoom];
                        let a = (c >> 24) as f32 / 255.0;
                        let blend = |sh: u32| {
                            let f = ((c >> sh) & 0xFF) as f32;
                            let b = ((bg >> sh) & 0xFF) as f32;
                            ((f * a + b * (1.0 - a)) as u32) << sh
                        };
                        img[(oy + 6 + y) * width + ox + 6 + x] = 0xFF000000 | blend(16) | blend(8) | blend(0);
                    }
                }
            }
        }
    }
    // BMP 32 bpp bottom-up.
    let mut f = Vec::new();
    let data = (width * height * 4) as u32;
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&(54 + data).to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    f.extend_from_slice(&54u32.to_le_bytes());
    f.extend_from_slice(&40u32.to_le_bytes());
    f.extend_from_slice(&(width as i32).to_le_bytes());
    f.extend_from_slice(&(height as i32).to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&32u16.to_le_bytes());
    f.extend_from_slice(&[0; 24]);
    for y in (0..height).rev() {
        for x in 0..width {
            f.extend_from_slice(&img[y * width + x].to_le_bytes());
        }
    }
    std::fs::write(&out, f).unwrap();
}
