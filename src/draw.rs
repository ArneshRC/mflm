use crate::buffer::{Buffer, BufferError};
use crate::color::Color;

use cairo::{Context, Format, ImageSurface};
use pangocairo::functions as pangocairo;
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DrawError {
    #[error("buffer error: {0}")]
    Buffer(#[from] BufferError),

    #[error("cairo/pango error: {0}")]
    Render(String),
}

pub struct Font {
    desc: pango::FontDescription,
    size_px: f32,
}

impl Font {
    pub fn new(desc: &str, size_px: f32) -> Font {
        let mut font_desc = pango::FontDescription::from_string(desc);
        // Treat the configured string as a Pango font description, but keep
        // size controlled by the caller to preserve existing layout.
        font_desc.set_absolute_size((size_px as f64) * (pango::SCALE as f64));
        Font {
            desc: font_desc,
            size_px,
        }
    }

    fn render_to_surface(&self, bg: &Color, fg: &Color, text: &str) -> Result<(ImageSurface, i32, i32), DrawError> {
        // Measure text using a tiny temporary surface.
        let tmp = ImageSurface::create(Format::ARgb32, 1, 1)
            .map_err(|e| DrawError::Render(format!("failed to create cairo surface: {e:?}")))?;
        let tmp_ctx = Context::new(&tmp)
            .map_err(|e| DrawError::Render(format!("failed to create cairo context: {e:?}")))?;
        let layout = pangocairo::create_layout(&tmp_ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);
        let (mut w, mut h) = layout.pixel_size();
        w = w.max(1);
        h = h.max(1);

        let surface = ImageSurface::create(Format::ARgb32, w, h)
            .map_err(|e| DrawError::Render(format!("failed to create cairo surface: {e:?}")))?;
        let ctx = Context::new(&surface)
            .map_err(|e| DrawError::Render(format!("failed to create cairo context: {e:?}")))?;

        let (br, bgc, bb, ba) = bg.as_rgba_f32();
        ctx.set_source_rgba(br, bgc, bb, ba);
        ctx.paint()
            .map_err(|e| DrawError::Render(format!("failed to paint background: {e:?}")))?;

        let layout = pangocairo::create_layout(&ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);

        let (fr, fgc, fb, fa) = fg.as_rgba_f32();
        ctx.set_source_rgba(fr, fgc, fb, fa);
        pangocairo::show_layout(&ctx, &layout);

        Ok((surface, w, h))
    }

    pub fn draw_text(
        &self,
        buf: &mut Buffer<'_>,
        bg: &Color,
        c: &Color,
        s: &str,
    ) -> Result<(u32, u32), DrawError> {
        // The existing UI expects a fixed "font height" in pixels; keep that.
        let (mut surface, w, h) = self.render_to_surface(bg, c, s)?;
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface
            .data()
            .map_err(|e| DrawError::Render(format!("failed to access cairo surface data: {e:?}")))?;

        // Cairo ARgb32 is native-endian premultiplied alpha; on little-endian
        // it is stored as BGRA bytes, which matches how we already write u32
        // pixels (ARGB value written as native u32).
        let bounds = buf.get_bounds();
        let max_w = (w as u32).min(bounds.2);
        let max_h = (h as u32).min(bounds.3);

        for y in 0..max_h {
            for x in 0..max_w {
                let off = (y as usize * stride) + (x as usize * 4);
                if off + 3 >= data.len() {
                    continue;
                }
                let b = data[off];
                let g = data[off + 1];
                let r = data[off + 2];
                let a = data[off + 3];
                let argb = u32::from_be_bytes([a, r, g, b]);
                buf.put_argb8888((x, y), argb)?;
            }
        }

        Ok((w as u32, self.size_px.max(h as f32) as u32))
    }

    pub fn auto_draw_text(
        &mut self,
        buf: &mut Buffer<'_>,
        bg: &Color,
        c: &Color,
        s: &str,
    ) -> Result<(u32, u32), DrawError> {
        self.draw_text(buf, bg, c, s)
    }
}

pub fn draw_box(buf: &mut Buffer<'_>, c: &Color, dim: (u32, u32)) -> Result<(), BufferError> {
    for x in 0..dim.0 {
        let _ = buf.put((x, 0), c);
        let _ = buf.put((x, dim.1 - 1), c);
    }
    for y in 0..dim.1 {
        buf.put((0, y), c)?;
        buf.put((dim.0 - 1, y), c)?;
    }

    Ok(())
}
