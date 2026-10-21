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

    fn single_line_layout(
        &self,
        text: &str
    ) -> Result<pango::Layout, DrawError> {
        let tmp = ImageSurface::create(Format::ARgb32, 1, 1).map_err(|e| {
            DrawError::Render(format!("failed to create cairo surface: {e:?}"))
        })?;
        let ctx = Context::new(&tmp).map_err(|e| {
            DrawError::Render(format!("failed to create cairo context: {e:?}"))
        })?;
        let layout = pangocairo::create_layout(&ctx);
        layout.set_font_description(Some(&self.desc));
        layout.set_text(text);
        Ok(layout)
    }

    /// Pixel size of `text` laid out on a single line.
    pub fn measure(&self, text: &str) -> Result<(u32, u32), DrawError> {
        let (w, h) = self.single_line_layout(text)?.pixel_size();
        Ok((w.max(0) as u32, h.max(1) as u32))
    }

    /// Distance from the top of `text`'s line to its baseline, in pixels.
    pub fn baseline(&self, text: &str) -> Result<u32, DrawError> {
        let baseline = self.single_line_layout(text)?.baseline();
        Ok((baseline / pango::SCALE).max(0) as u32)
    }

    /// Number of glyphs in `text` that no installed font provides. Pango
    /// draws those as hex boxes.
    pub fn missing_glyphs(&self, text: &str) -> Result<u32, DrawError> {
        let count = self.single_line_layout(text)?.unknown_glyphs_count();
        Ok(count.max(0) as u32)
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
        let fg = self.colors.text;

        let form_fill =
            if box_color.as_argb8888() == self.colors.muted.as_argb8888() {
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

        let heading = self.header.render()?;

        self.heading_font.auto_draw_text_centered(
            &mut buf.offset((0, self.header_offset))?,
            &bg,
            &fg,
            &heading
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
                self.colors.accent
            } else {
                self.colors.muted
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
                self.colors.accent
            } else {
                self.colors.muted
            };
            Self::draw_underline(&mut row, layout.w, layout.row_h, &c);
        }

        self.draw_status()?;
        self.draw_power_row()?;

        self.should_refresh = true;

        Ok(())
    }

    pub(crate) fn draw_power_row(&mut self) -> Result<(), crate::Error> {
        let layout = match self.power_row_layout() {
            Some(layout) => layout,
            None => return Ok(())
        };

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        for (i, (item, x)) in self.power_items.iter().zip(layout.xs.iter().copied()).enumerate()
        {
            let (bg, fg) = if self.power_armed == Some(i) {
                (self.colors.confirm, self.colors.background)
            } else {
                (self.colors.background, self.colors.text)
            };

            let mut cell = buf.subdimensions((x, layout.y, item.w, layout.h))?;
            cell.memset(&bg);

            let text_h = item.text_h.min(layout.h);
            let mut text = cell.subdimensions((
                0,
                (layout.h - text_h) / 2,
                item.w,
                text_h
            ))?;
            self.power_font
                .auto_draw_text_centered(&mut text, &bg, &fg, &item.text)?;
        }

        if let Some(hint_y) = layout.hint_y {
            let bg = self.colors.background;
            let (hint_w, hint_h) = self.power_hint_size;
            buf.subdimensions((0, hint_y, self.screen_size.0, hint_h))?
                .memset(&bg);

            if let Some(i) = self.power_armed {
                // centered under the armed action, kept on screen
                let hint_w = hint_w.min(self.screen_size.0);
                let center = layout.xs[i] + self.power_items[i].w / 2;
                let x = center
                    .saturating_sub(hint_w / 2)
                    .min(self.screen_size.0 - hint_w);
                let fg = self.colors.muted;
                self.power_hint_font.auto_draw_text_centered(
                    &mut buf.subdimensions((x, hint_y, hint_w, hint_h))?,
                    &bg,
                    &fg,
                    crate::power::HINT
                )?;
            }
        }

        self.should_refresh = true;

        Ok(())
    }

    pub(crate) fn draw_status(&mut self) -> Result<(), crate::Error> {
        let (y, h) = match self.status_layout() {
            Some(layout) => layout,
            None => return Ok(())
        };

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let mut line = buf.subdimensions((0, y, self.screen_size.0, h))?;
        let bg = self.colors.background;
        line.memset(&bg);

        let (fg, text) = match &self.status {
            Some(crate::Status::Info(text)) => (self.colors.muted, text),
            Some(crate::Status::Error(text)) => (self.colors.error, text),
            None => {
                self.should_refresh = true;
                return Ok(());
            }
        };
        if let Some(font) = self.status_font.as_mut() {
            font.auto_draw_text_centered(&mut line, &bg, &fg, text)?;
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
            self.colors.accent
        } else {
            self.colors.text
        };

        let (left, right) =
            (self.session_left_arrow.as_str(), self.session_right_arrow.as_str());
        let with_arrows = |name: &str| match (left, right) {
            ("", "") => name.to_string(),
            (l, "") => format!("{l}  {name}"),
            ("", r) => format!("{name}  {r}"),
            (l, r) => format!("{l}  {name}  {r}")
        };

        // Shorten the name with an ellipsis until the row fits on one line.
        let session_name = &self.targets[self.target_index].name;
        let mut text = with_arrows(session_name);
        if self.main_font.measure(&text)?.0 > layout.w {
            let mut cuts: Vec<usize> =
                session_name.char_indices().map(|(i, _)| i).collect();
            cuts.reverse();
            for cut in cuts {
                text = with_arrows(&format!(
                    "{}…",
                    session_name[..cut].trim_end()
                ));
                if self.main_font.measure(&text)?.0 <= layout.w {
                    break;
                }
            }
        }

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
        let y = match self.form_layout().username_y {
            Some(y) => y,
            None => return Ok(())
        };
        let focused = self.mode == crate::Mode::EditingUsername;
        let icon = self.field_icons.username.clone();
        self.draw_field(y, username, focused, redraw, icon)
    }

    pub(crate) fn draw_password(
        &mut self,
        password: &str,
        redraw: bool
    ) -> Result<(), crate::Error> {
        let y = self.form_layout().password_y;

        let mut stars = String::new();
        for _ in 0..password.chars().count() {
            stars.push_str(&self.password_mask);
        }

        let focused = self.mode == crate::Mode::EditingPassword;
        let icon = self.field_icons.password.clone();
        self.draw_field(y, &stars, focused, redraw, icon)
    }

    /// Input row at `y` showing `icon` and `text` above an underline.
    /// `redraw` clears the row first, needed when the text got shorter.
    fn draw_field(
        &mut self,
        y: u32,
        text: &str,
        focused: bool,
        redraw: bool,
        icon: Option<crate::field_icon::FieldIcon>
    ) -> Result<(), crate::Error> {
        let layout = self.form_layout();

        let mut buf = crate::buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf =
            buf.subdimensions((layout.x, y, layout.w, layout.row_h))?;
        let bg = self.colors.background;
        if redraw {
            buf.memset(&bg);
        }

        let fg = if focused {
            self.colors.accent
        } else {
            self.colors.text
        };
        // the icon is a label, so it follows the underline, not the text
        let border = if focused {
            self.colors.accent
        } else {
            self.colors.muted
        };

        let align = match self.text_align {
            crate::settings::TextAlign::Left => pango::Alignment::Left,
            crate::settings::TextAlign::Center => pango::Alignment::Center,
            crate::settings::TextAlign::Right => pango::Alignment::Right
        };

        let cols = crate::layout::field_columns(
            layout.w,
            self.form_padding,
            self.field_icons.col_w,
            self.text_align
        );

        // Typing never moves the icon, so it only needs drawing on redraw.
        if let (true, Some(icon)) = (redraw, icon) {
            let dy = icon.dy.min(layout.row_h);
            let mut cell = buf.subdimensions((
                cols.icon_x,
                dy,
                cols.text_x - cols.icon_x,
                layout.row_h - dy
            ))?;
            self.icon_font.auto_draw_text_aligned(
                &mut cell,
                &bg,
                &border,
                &icon.text,
                pango::Alignment::Left
            )?;
        }

        let dy = self.field_icons.text_dy.min(layout.row_h);
        let mut inner = buf.subdimensions((
            cols.text_x,
            dy,
            cols.text_w,
            layout.row_h - dy
        ))?;
        self.main_font
            .auto_draw_text_aligned(&mut inner, &bg, &fg, text, align)?;

        Self::draw_underline(&mut buf, layout.w, layout.row_h, &border);

        self.should_refresh = true;

        Ok(())
    }
}
