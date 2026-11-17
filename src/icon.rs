//! Iconos generados en memoria (sin archivos .ico): símbolo del dispositivo arriba,
//! porcentaje abajo y un rayo verde cuando está cargando.
//!
//! Las formas se describen en una rejilla de 16×16 "unidades" y se rasterizan con
//! supermuestreo 4×4 (suavizado). Los dígitos usan una fuente de píxeles 3×5 alineada
//! a píxel para que el número sea nítido incluso a 16 px.

use crate::ffi::*;
use core::ffi::c_void;
use std::ptr::null_mut;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Keyboard,
    Headset,
    Mouse,
    /// Genérico (mando u otros) y estado "sin dispositivos".
    Battery,
}

const RED: u32 = 0xFFF8_5149;
const GREEN: u32 = 0xFF3D_DC68;
const GRAY: u32 = 0xFF9E_9E9E;
const CLEAR: u32 = 0;
const SS: usize = 4; // supermuestreo por eje

const DIGITS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    [0b000, 0b000, 0b111, 0b000, 0b000], // -
];

/// Rayo de carga verde (unidades), esquina superior derecha.
const BOLT: [(f32, f32); 7] =
    [(12.2, 0.0), (9.2, 5.0), (11.6, 5.0), (10.6, 9.0), (15.4, 3.6), (12.9, 3.6), (14.4, 0.0)];

struct Canvas {
    n: usize,  // lado en subpíxeles
    u: f32,    // subpíxeles por unidad
    px: Vec<u32>,
}

impl Canvas {
    fn new(s: usize) -> Self {
        let n = s * SS;
        Self { n, u: n as f32 / 16.0, px: vec![CLEAR; n * n] }
    }

    /// Pinta `color` en cada subpíxel dentro de la caja (unidades) que cumpla `inside`.
    fn shade(&mut self, bx: (f32, f32, f32, f32), color: u32, inside: impl Fn(f32, f32) -> bool) {
        let n = self.n as f32;
        let sx0 = (bx.0 * self.u).floor().clamp(0.0, n) as usize;
        let sy0 = (bx.1 * self.u).floor().clamp(0.0, n) as usize;
        let sx1 = (bx.2 * self.u).ceil().clamp(0.0, n) as usize;
        let sy1 = (bx.3 * self.u).ceil().clamp(0.0, n) as usize;
        for sy in sy0..sy1 {
            for sx in sx0..sx1 {
                let (x, y) = ((sx as f32 + 0.5) / self.u, (sy as f32 + 0.5) / self.u);
                if inside(x, y) {
                    self.px[sy * self.n + sx] = color;
                }
            }
        }
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32) {
        self.shade((x0, y0, x1, y1), c, |x, y| x >= x0 && x < x1 && y >= y0 && y < y1);
    }

    fn round_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: u32) {
        self.shade((x0, y0, x1, y1), c, |x, y| {
            let dx = (x0 + r - x).max(x - (x1 - r)).max(0.0);
            let dy = (y0 + r - y).max(y - (y1 - r)).max(0.0);
            x >= x0 && x < x1 && y >= y0 && y < y1 && dx * dx + dy * dy <= r * r
        });
    }

    /// Semianillo superior (diadema de los auriculares).
    fn arch(&mut self, cx: f32, cy: f32, r_out: f32, r_in: f32, c: u32) {
        self.shade((cx - r_out, cy - r_out, cx + r_out, cy), c, |x, y| {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            y <= cy && d <= r_out && d >= r_in
        });
    }

    /// Polígono; `grow` > 0 lo engorda (para recortar un contorno alrededor).
    fn polygon(&mut self, pts: &[(f32, f32)], grow: f32, c: u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        self.shade((x0 - grow, y0 - grow, x1 + grow, y1 + grow), c, |x, y| {
            point_in_polygon(pts, x, y) || (grow > 0.0 && dist_to_polygon(pts, x, y) <= grow)
        });
    }

    /// Rectángulo en píxeles de salida (para los dígitos, alineados a píxel).
    fn pixel_rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: u32) {
        for sy in (y * SS)..((y + h) * SS).min(self.n) {
            for sx in (x * SS)..((x + w) * SS).min(self.n) {
                self.px[sy * self.n + sx] = c;
            }
        }
    }

    /// Promedia cada bloque SS×SS (alfa ponderado) → ARGB sin premultiplicar.
    fn finish(&self, s: usize) -> Vec<u32> {
        let mut out = vec![0u32; s * s];
        for py in 0..s {
            for px in 0..s {
                let (mut a, mut r, mut g, mut b) = (0u32, 0u32, 0u32, 0u32);
                for sy in 0..SS {
                    for sx in 0..SS {
                        let c = self.px[(py * SS + sy) * self.n + px * SS + sx];
                        let ca = c >> 24;
                        a += ca;
                        r += ((c >> 16) & 0xFF) * ca;
                        g += ((c >> 8) & 0xFF) * ca;
                        b += (c & 0xFF) * ca;
                    }
                }
                if a > 0 {
                    let alpha = a / (SS * SS) as u32;
                    out[py * s + px] = (alpha << 24) | ((r / a) << 16) | ((g / a) << 8) | (b / a);
                }
            }
        }
        out
    }
}

fn point_in_polygon(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let ((xi, yi), (xj, yj)) = (pts[i], pts[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_to_polygon(pts: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut best = f32::MAX;
    for i in 0..pts.len() {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % pts.len()];
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (cx, cy) = (ax + t * dx - x, ay + t * dy - y);
        best = best.min((cx * cx + cy * cy).sqrt());
    }
    best
}

fn draw_glyph(cv: &mut Canvas, glyph: Glyph, fg: u32, level: Option<u8>) {
    match glyph {
        Glyph::Keyboard => {
            cv.round_rect(0.0, 1.0, 15.0, 8.0, 1.2, fg);
            cv.round_rect(1.0, 2.0, 14.0, 7.0, 0.4, CLEAR);
            for x in [2.0, 4.0, 6.0, 8.0, 10.0, 12.0] {
                cv.rect(x, 3.0, x + 1.0, 4.0, fg);
            }
            cv.rect(2.0, 5.0, 3.0, 6.0, fg);
            cv.rect(4.0, 5.0, 11.0, 6.0, fg); // barra espaciadora
            cv.rect(12.0, 5.0, 13.0, 6.0, fg);
        }
        Glyph::Headset => {
            // Diadema fina y estrecha + almohadillas redondeadas (proporción ~1:1).
            cv.arch(8.0, 6.6, 5.4, 4.4, fg);
            cv.rect(2.6, 6.5, 3.6, 7.2, fg); // bajada de la diadema hasta las almohadillas
            cv.rect(12.4, 6.5, 13.4, 7.2, fg);
            cv.round_rect(1.8, 5.0, 5.2, 9.0, 1.1, fg);
            cv.round_rect(10.8, 5.0, 14.2, 9.0, 1.1, fg);
        }
        Glyph::Mouse => {
            cv.round_rect(4.0, 0.3, 12.0, 8.8, 3.6, fg);
            cv.round_rect(5.0, 1.3, 11.0, 7.8, 2.7, CLEAR);
            cv.rect(7.5, 0.3, 8.5, 4.0, fg); // separación de botones
            cv.rect(4.5, 3.6, 11.5, 4.4, fg);
        }
        Glyph::Battery => {
            cv.round_rect(0.5, 2.0, 14.0, 8.0, 1.0, fg);
            cv.round_rect(1.5, 3.0, 13.0, 7.0, 0.3, CLEAR);
            cv.rect(14.0, 3.8, 15.5, 6.2, fg);
            if let Some(l) = level {
                cv.rect(2.5, 4.0, 2.5 + 9.5 * l.min(100) as f32 / 100.0, 6.0, fg);
            }
        }
    }
}

/// Píxeles ARGB (sin premultiplicar) de un icono de `size`×`size`.
pub fn pixels(size: i32, glyph: Glyph, level: Option<u8>, bolt: bool, charging: bool, light: bool) -> (usize, Vec<u32>) {
    let s = size.clamp(16, 64) as usize;
    let fg = if light { 0xFF1A_1A1A } else { 0xFFFF_FFFF };
    let mut cv = Canvas::new(s);

    draw_glyph(&mut cv, glyph, fg, level);
    if bolt {
        cv.polygon(&BOLT, 0.9, CLEAR); // contorno transparente para separarlo del símbolo
        cv.polygon(&BOLT, 0.0, GREEN);
    }

    // Número: franja inferior (~7/16 del alto).
    let (text, color) = match level {
        Some(l) => (l.min(100).to_string(), if l < 20 && !charging { RED } else { fg }),
        None => ("--".to_string(), GRAY),
    };
    let num_top = s * 9 / 16;
    let num_h = s - num_top;
    let cell_h = (num_h / 5).max(1);
    let n = text.len();
    let cols = n * 3 + (n - 1);
    // Celdas cuadradas (número pequeño); solo a 16-20 px se ensanchan a 2 para leerse.
    let cell_w = (s / cols).min(cell_h.max(2)).max(1);
    let x0 = s.saturating_sub(cols * cell_w) / 2;
    let y0 = s - 5 * cell_h; // pegado abajo: deja aire entre símbolo y número
    for (i, ch) in text.chars().enumerate() {
        let g = &DIGITS[ch.to_digit(10).map_or(10, |d| d as usize)];
        for (row, bits) in g.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    cv.pixel_rect(x0 + (i * 4 + col) * cell_w, y0 + row * cell_h, cell_w, cell_h, color);
                }
            }
        }
    }
    (s, cv.finish(s))
}

pub fn render(size: i32, glyph: Glyph, level: Option<u8>, bolt: bool, charging: bool, light: bool) -> HICON {
    let (s, px) = pixels(size, glyph, level, bolt, charging, light);
    unsafe { make_hicon(s as i32, &px) }
}

unsafe fn make_hicon(s: i32, px: &[u32]) -> HICON {
    let bmi = BITMAPINFO {
        header: BITMAPINFOHEADER {
            size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            width: s,
            height: -s, // top-down
            planes: 1,
            bit_count: 32,
            compression: BI_RGB,
            size_image: 0,
            x_ppm: 0,
            y_ppm: 0,
            clr_used: 0,
            clr_important: 0,
        },
        colors: [0],
    };
    let mut bits: *mut c_void = null_mut();
    let color = CreateDIBSection(null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if color.is_null() || bits.is_null() {
        if !color.is_null() {
            DeleteObject(color);
        }
        return null_mut();
    }
    std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u32, px.len());

    // Máscara AND a cero: la transparencia la decide el canal alfa del DIB.
    let stride = ((s as usize + 15) / 16) * 2;
    let mask_bits = vec![0u8; stride * s as usize];
    let mask = CreateBitmap(s, s, 1, 1, mask_bits.as_ptr() as *const c_void);

    let info = ICONINFO { is_icon: 1, x_hotspot: 0, y_hotspot: 0, mask, color };
    let icon = CreateIconIndirect(&info);
    DeleteObject(color);
    if !mask.is_null() {
        DeleteObject(mask);
    }
    icon
}
