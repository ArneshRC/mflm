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
    pub(crate) xs: Vec<u32>
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

        Some(PowerRowLayout { y, h, xs })
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
