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

    fn render_to_surface_centered(
        &self,
        bg: &Color,
        fg: &Color,
        text: &str,
        width_px: i32,
    ) -> Result<(ImageSurface, i32, i32), DrawError> {
        let width_px = width_px.max(1);

        let tmp = ImageSurface::create(Format::ARgb32, 1, 1)
            .map_err(|e| DrawError::Render(format!("failed to create cairo surface: {e:?}")))?;
        let tmp_ctx = Context::new(&tmp)
            .map_err(|e| DrawError::Render(format!("failed to create cairo context: {e:?}")))?;

        let layout = pangocairo::create_layout(&tmp_ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);
        layout.set_width(width_px * pango::SCALE);
        layout.set_alignment(pango::Alignment::Center);
        let (_w, mut h) = layout.pixel_size();
        h = h.max(1);

        let surface = ImageSurface::create(Format::ARgb32, width_px, h)
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
        layout.set_width(width_px * pango::SCALE);
        layout.set_alignment(pango::Alignment::Center);

        let (fr, fgc, fb, fa) = fg.as_rgba_f32();
        ctx.set_source_rgba(fr, fgc, fb, fa);
        pangocairo::show_layout(&ctx, &layout);

        Ok((surface, width_px, h))
    }

    pub fn auto_draw_text_centered(
        &mut self,
        buf: &mut Buffer<'_>,
        bg: &Color,
        c: &Color,
        s: &str,
    ) -> Result<(u32, u32), DrawError> {
        let bounds = buf.get_bounds();
        let width_px = bounds.2 as i32;

        let (mut surface, w, h) = self.render_to_surface_centered(bg, c, s, width_px)?;
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface
            .data()
            .map_err(|e| DrawError::Render(format!("failed to access cairo surface data: {e:?}")))?;

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
}
