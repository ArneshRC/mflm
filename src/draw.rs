use crate::{
    buffer::{Buffer, BufferError},
    color::Color
};

use cairo::{Context, Format, ImageSurface};
use pangocairo::functions as pangocairo;
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DrawError {
    #[error("buffer error: {0}")]
    Buffer(#[from] BufferError),

    #[error("cairo/pango error: {0}")]
    Render(String)
}

pub struct Font {
    desc: pango::FontDescription,
    size_px: f32
}

impl Font {
    pub fn new(desc: &str, size_px: f32) -> Font {
        let mut font_desc = pango::FontDescription::from_string(desc);
        // Treat the configured string as a Pango font description, but keep
        // size controlled by the caller to preserve existing layout.
        font_desc.set_absolute_size((size_px as f64) * (pango::SCALE as f64));
        Font {
            desc: font_desc,
            size_px
        }
    }

    fn render_to_surface_aligned(
        &self,
        bg: &Color,
        fg: &Color,
        text: &str,
        width_px: i32,
        alignment: pango::Alignment
    ) -> Result<(ImageSurface, i32, i32), DrawError> {
        let width_px = width_px.max(1);

        let tmp = ImageSurface::create(Format::ARgb32, 1, 1).map_err(|e| {
            DrawError::Render(format!("failed to create cairo surface: {e:?}"))
        })?;
        let tmp_ctx = Context::new(&tmp).map_err(|e| {
            DrawError::Render(format!("failed to create cairo context: {e:?}"))
        })?;

        let layout = pangocairo::create_layout(&tmp_ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);
        layout.set_width(width_px * pango::SCALE);
        layout.set_alignment(alignment);
        let (_w, mut h) = layout.pixel_size();
        h = h.max(1);

        let surface = ImageSurface::create(Format::ARgb32, width_px, h)
            .map_err(|e| {
                DrawError::Render(format!(
                    "failed to create cairo surface: {e:?}"
                ))
            })?;
        let ctx = Context::new(&surface).map_err(|e| {
            DrawError::Render(format!("failed to create cairo context: {e:?}"))
        })?;

        let (br, bgc, bb, ba) = bg.as_rgba_f32();
        ctx.set_source_rgba(br, bgc, bb, ba);
        ctx.paint().map_err(|e| {
            DrawError::Render(format!("failed to paint background: {e:?}"))
        })?;

        let layout = pangocairo::create_layout(&ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);
        layout.set_width(width_px * pango::SCALE);
        layout.set_alignment(alignment);

        let (fr, fgc, fb, fa) = fg.as_rgba_f32();
        ctx.set_source_rgba(fr, fgc, fb, fa);
        pangocairo::show_layout(&ctx, &layout);

        Ok((surface, width_px, h))
    }

    pub fn auto_draw_text_aligned(
        &mut self,
        buf: &mut Buffer<'_>,
        bg: &Color,
        c: &Color,
        s: &str,
        alignment: pango::Alignment
    ) -> Result<(u32, u32), DrawError> {
        let bounds = buf.get_bounds();
        let width_px = bounds.2 as i32;

        let (mut surface, w, h) =
            self.render_to_surface_aligned(bg, c, s, width_px, alignment)?;
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface.data().map_err(|e| {
            DrawError::Render(format!(
                "failed to access cairo surface data: {e:?}"
            ))
        })?;

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

    pub fn auto_draw_text_centered(
        &mut self,
        buf: &mut Buffer<'_>,
        bg: &Color,
        c: &Color,
        s: &str
    ) -> Result<(u32, u32), DrawError> {
        self.auto_draw_text_aligned(buf, bg, c, s, pango::Alignment::Center)
    }
}

impl crate::LoginManager<'_> {
    pub(crate) fn refresh(&mut self) {
        if self.should_refresh {
            self.should_refresh = false;
            let mut screeninfo = self.var_screen_info.clone();
            screeninfo.activate |=
                crate::FB_ACTIVATE_NOW | crate::FB_ACTIVATE_FORCE;
            if let Err(e) = framebuffer::Framebuffer::put_var_screeninfo(
                self.device,
                &screeninfo
            ) {
                log::error!("Failed to refresh framebuffer: {e}");
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        buf.memset(&self.colors.background);
        self.should_refresh = true;
    }

    fn draw_underline(
        row: &mut crate::buffer::Buffer<'_>,
        row_w: u32,
        row_h: u32,
        color: &Color
    ) {
        let thickness = 4u32.min(row_h.max(1));
        let underline_w = (row_w).max(16).min(row_w);
        let start_x = (row_w.saturating_sub(underline_w)) / 2;
        let start_y = row_h.saturating_sub(thickness);

        for y in start_y..row_h {
            for x in start_x..start_x.saturating_add(underline_w) {
                let _ = row.put((x, y), color);
            }
        }
    }

    pub(crate) fn draw_bg(
        &mut self,
        box_color: &Color
    ) -> Result<(), crate::Error> {
        let layout = self.form_layout();
        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let bg = self.colors.background;
        let fg = self.colors.foreground;

        let form_fill =
            if box_color.as_argb8888() == self.colors.neutral.as_argb8888() {
                bg
            } else {
                *box_color
            };

        {
            let mut form = buf.subdimensions((
                layout.x,
                layout.y,
                layout.w,
                layout.total_h
            ))?;
            form.memset(&form_fill);
        }

        let hostname = hostname::get()?.to_string_lossy().into_owned();

        self.heading_font.auto_draw_text_centered(
            &mut buf.offset((0, 32))?,
            &bg,
            &fg,
            &format!("Welcome to {hostname}")
        )?;

        // Underlines (username/password). Selected field uses selected color.
        if let Some(y_username) = layout.username_y {
            let mut row = buf.subdimensions((
                layout.x,
                y_username,
                layout.w,
                layout.row_h
            ))?;
            let c = if self.mode == crate::Mode::EditingUsername {
                self.colors.selected
            } else {
                self.colors.neutral
            };
            Self::draw_underline(&mut row, layout.w, layout.row_h, &c);
        }

        {
            let mut row = buf.subdimensions((
                layout.x,
                layout.password_y,
                layout.w,
                layout.row_h
            ))?;
            let c = if self.mode == crate::Mode::EditingPassword {
                self.colors.selected
            } else {
                self.colors.neutral
            };
            Self::draw_underline(&mut row, layout.w, layout.row_h, &c);
        }

        self.should_refresh = true;

        Ok(())
    }

    pub(crate) fn draw_target(&mut self) -> Result<(), crate::Error> {
        let layout = self.form_layout();
        let y = match layout.session_y {
            Some(y) => y,
            None => return Ok(())
        };

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf =
            buf.subdimensions((layout.x, y, layout.w, layout.row_h))?;
        let bg = self.colors.background;
        buf.memset(&bg);

        let fg = if self.mode == crate::Mode::SelectingSession {
            self.colors.selected
        } else {
            self.colors.foreground
        };

        let session_name = &self.targets[self.target_index].name;
        let text = match (
            self.session_left_arrow.as_str(),
            self.session_right_arrow.as_str()
        ) {
            ("", "") => session_name.to_string(),
            (l, "") => format!("{l}  {session_name}"),
            ("", r) => format!("{session_name}  {r}"),
            (l, r) => format!("{l}  {session_name}  {r}")
        };

        self.main_font
            .auto_draw_text_centered(&mut buf, &bg, &fg, &text)?;

        self.should_refresh = true;

        Ok(())
    }

    pub(crate) fn draw_username(
        &mut self,
        username: &str,
        redraw: bool
    ) -> Result<(), crate::Error> {
        let layout = self.form_layout();
        let y = match layout.username_y {
            Some(y) => y,
            None => return Ok(())
        };

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf =
            buf.subdimensions((layout.x, y, layout.w, layout.row_h))?;
        let bg = self.colors.background;
        if redraw {
            buf.memset(&bg);
        }

        let fg = if self.mode == crate::Mode::EditingUsername {
            self.colors.selected
        } else {
            self.colors.foreground
        };

        let align = match self.text_align {
            crate::settings::TextAlign::Left => pango::Alignment::Left,
            crate::settings::TextAlign::Center => pango::Alignment::Center,
            crate::settings::TextAlign::Right => pango::Alignment::Right
        };

        let margin = self.input_margin_px.min(layout.w / 2);
        if margin > 0 {
            let inner_w = layout.w.saturating_sub(margin * 2);
            let mut inner = buf.subdimensions((margin, 0, inner_w, layout.row_h))?;
            self.main_font
                .auto_draw_text_aligned(&mut inner, &bg, &fg, username, align)?;
        } else {
            self.main_font
                .auto_draw_text_aligned(&mut buf, &bg, &fg, username, align)?;
        }

        let border = if self.mode == crate::Mode::EditingUsername {
            self.colors.selected
        } else {
            self.colors.neutral
        };
        Self::draw_underline(&mut buf, layout.w, layout.row_h, &border);

        self.should_refresh = true;

        Ok(())
    }

    pub(crate) fn draw_password(
        &mut self,
        password: &str,
        redraw: bool
    ) -> Result<(), crate::Error> {
        let layout = self.form_layout();
        let y = layout.password_y;

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf =
            buf.subdimensions((layout.x, y, layout.w, layout.row_h))?;
        let bg = self.colors.background;
        if redraw {
            buf.memset(&bg);
        }

        let mut stars = String::new();
        for _ in 0..password.len() {
            stars.push_str(&self.password_char);
        }

        let fg = if self.mode == crate::Mode::EditingPassword {
            self.colors.selected
        } else {
            self.colors.foreground
        };

        let align = match self.text_align {
            crate::settings::TextAlign::Left => pango::Alignment::Left,
            crate::settings::TextAlign::Center => pango::Alignment::Center,
            crate::settings::TextAlign::Right => pango::Alignment::Right
        };

        let margin = self.input_margin_px.min(layout.w / 2);
        if margin > 0 {
            let inner_w = layout.w.saturating_sub(margin * 2);
            let mut inner = buf.subdimensions((margin, 0, inner_w, layout.row_h))?;
            self.main_font
                .auto_draw_text_aligned(&mut inner, &bg, &fg, &stars, align)?;
        } else {
            self.main_font
                .auto_draw_text_aligned(&mut buf, &bg, &fg, &stars, align)?;
        }

        // Bottom border under password input.
        let border = if self.mode == crate::Mode::EditingPassword {
            self.colors.selected
        } else {
            self.colors.neutral
        };
        Self::draw_underline(&mut buf, layout.w, layout.row_h, &border);

        self.should_refresh = true;

        Ok(())
    }
}
