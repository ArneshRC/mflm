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

impl<'a> crate::LoginManager<'a> {
    pub(crate) fn form_layout(&self) -> FormLayout {
        let row_h = self.row_h;
        let gap = self.gap_px;

        let show_session = !self.lock_target;
        let show_username = self.forced_username.is_none();
        let rows = (show_session as u32) + (show_username as u32) + 1;
        let total_h = rows * row_h + rows.saturating_sub(1) * gap;

        let margin_x = 32;
        let max_w = self.screen_size.0.saturating_sub(margin_x * 2).max(1);
        let w = self.dimensions.0.min(max_w).max(1);

        let x = (self.screen_size.0.saturating_sub(w)) / 2;
        let y = (self.screen_size.1.saturating_sub(total_h)) / 2;

        let mut cur_y = y;
        let session_y = if show_session {
            let out = cur_y;
            cur_y = cur_y.saturating_add(row_h + gap);
            Some(out)
        } else {
            None
        };

        let username_y = if show_username {
            let out = cur_y;
            cur_y = cur_y.saturating_add(row_h + gap);
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
