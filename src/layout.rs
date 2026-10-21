#[derive(Clone, Copy, Debug)]
pub(crate) struct FormLayout {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) row_h: u32,
    pub(crate) total_h: u32,
    pub(crate) session_y: Option<u32>,
    pub(crate) username_y: Option<u32>,
    pub(crate) password_y: u32
}

#[derive(Clone, Debug)]
pub(crate) struct PowerRowLayout {
    pub(crate) y: u32,
    pub(crate) h: u32,
    /// Left edge of each item, in `power_items` order.
    pub(crate) xs: Vec<u32>,
    /// Top of the confirmation hint, or None if it does not fit.
    pub(crate) hint_y: Option<u32>
}

impl crate::LoginManager<'_> {
    /// Row of power actions centered below the form, or None if there are no
    /// actions or the row does not fit on screen.
    pub(crate) fn power_row_layout(&self) -> Option<PowerRowLayout> {
        if self.power_items.is_empty() {
            return None;
        }

        let form = self.form_layout();
        let y = form
            .y
            .checked_add(form.total_h)?
            .checked_add(self.power_offset_y_px)?;
        let h = self.power_item_h;
        if y.checked_add(h)? > self.screen_size.1 {
            return None;
        }

        let gaps = self
            .power_gap_px
            .checked_mul(self.power_items.len() as u32 - 1)?;
        let total_w = self
            .power_items
            .iter()
            .try_fold(gaps, |sum, item| sum.checked_add(item.w))?;
        if total_w > self.screen_size.0 {
            return None;
        }

        let mut x = (self.screen_size.0 - total_w) / 2;
        let xs = self
            .power_items
            .iter()
            .map(|item| {
                let out = x;
                x += item.w + self.power_gap_px;
                out
            })
            .collect();

        let hint_y = (y + h)
            .checked_add(self.power_hint_offset_y_px)
            .filter(|hint_y| {
                self.power_hint_size.1 > 0
                    && hint_y.checked_add(self.power_hint_size.1)
                        .is_some_and(|bottom| bottom <= self.screen_size.1)
            });

        Some(PowerRowLayout { y, h, xs, hint_y })
    }

    /// Top and height of the status line below the form, or None if it is
    /// hidden or does not fit on screen.
    pub(crate) fn status_layout(&self) -> Option<(u32, u32)> {
        self.status_font.as_ref()?;
        let form = self.form_layout();
        let y = form
            .y
            .checked_add(form.total_h)?
            .checked_add(self.status_offset_y_px)?;
        if y.checked_add(self.status_h)? > self.screen_size.1 {
            return None;
        }
        Some((y, self.status_h))
    }

    pub(crate) fn form_layout(&self) -> FormLayout {
        let row_h = self.row_h;
        let gap_below_session_px = self.gap_below_session_px;
        let gap_below_username_px = self.gap_below_username_px;

        let show_session = self.show_target_row();
        let show_username = self.show_username_row();

        let rows = (show_session as u32) + (show_username as u32) + 1;

        let gaps_h = {
            let mut sum = 0u32;
            if show_session {
                sum = sum.saturating_add(gap_below_session_px);
            }
            if show_username {
                sum = sum.saturating_add(gap_below_username_px);
            }
            sum
        };

        let total_h = rows * row_h + gaps_h;

        let margin_x = 32;
        let max_w = self.screen_size.0.saturating_sub(margin_x * 2).max(1);
        let w = self.dimensions.0.min(max_w).max(1);

        let x = (self.screen_size.0.saturating_sub(w)) / 2;
        let y = (self.screen_size.1.saturating_sub(total_h)) / 2;

        let mut cur_y = y;
        let session_y = if show_session {
            let out = cur_y;
            cur_y = cur_y.saturating_add(row_h + gap_below_session_px);
            Some(out)
        } else {
            None
        };

        let username_y = if show_username {
            let out = cur_y;
            cur_y = cur_y.saturating_add(row_h + gap_below_username_px);
            Some(out)
        } else {
            None
        };

        let password_y = cur_y;

        FormLayout {
            x,
            y,
            w,
            row_h,
            total_h,
            session_y,
            username_y,
            password_y
        }
    }
}

/// Horizontal split of an input row into an icon column starting at
/// `icon_x` and a text area of `text_w` starting at `text_x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FieldColumns {
    pub(crate) icon_x: u32,
    pub(crate) text_x: u32,
    pub(crate) text_w: u32
}

/// `icon_col_w` is the icon column including its gap, 0 for no icons.
pub(crate) fn field_columns(
    row_w: u32,
    margin: u32,
    icon_col_w: u32,
    align: crate::settings::TextAlign
) -> FieldColumns {
    let margin = margin.min(row_w / 2);
    let inner_w = row_w - margin * 2;
    // Centered text reserves the icon column on the right too, so it stays
    // centered on the form like the heading and session row.
    let reserved = match align {
        crate::settings::TextAlign::Center => icon_col_w.saturating_mul(2),
        _ => icon_col_w
    };
    FieldColumns {
        icon_x: margin,
        text_x: margin + icon_col_w.min(inner_w),
        text_w: inner_w.saturating_sub(reserved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TextAlign;

    fn cols(icon_x: u32, text_x: u32, text_w: u32) -> FieldColumns {
        FieldColumns {
            icon_x,
            text_x,
            text_w
        }
    }

    #[test]
    fn no_icons() {
        assert_eq!(field_columns(500, 15, 0, TextAlign::Left), cols(15, 15, 470));
        assert_eq!(field_columns(500, 15, 0, TextAlign::Center), cols(15, 15, 470));
    }

    #[test]
    fn left_and_right_reserve_column_once() {
        assert_eq!(field_columns(500, 15, 50, TextAlign::Left), cols(15, 65, 420));
        assert_eq!(field_columns(500, 15, 50, TextAlign::Right), cols(15, 65, 420));
    }

    #[test]
    fn center_reserves_column_on_both_sides() {
        assert_eq!(field_columns(500, 15, 50, TextAlign::Center), cols(15, 65, 370));
    }

    #[test]
    fn margin_capped_at_half_row() {
        assert_eq!(field_columns(20, 15, 0, TextAlign::Left), cols(10, 10, 0));
    }

    #[test]
    fn icons_wider_than_row_stay_in_bounds() {
        let c = field_columns(100, 10, 500, TextAlign::Center);
        assert_eq!(c, cols(10, 90, 0));
        assert!(c.text_x + c.text_w <= 100);
    }

    #[test]
    fn zero_width_row() {
        assert_eq!(field_columns(0, 15, 50, TextAlign::Left), cols(0, 0, 0));
    }
}
