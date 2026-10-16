use log::{error, info, warn};

use crate::vt::Key;

impl crate::LoginManager<'_> {
    fn mode_allowed(&self, mode: crate::Mode) -> bool {
        match mode {
            crate::Mode::SelectingSession => self.show_target_row(),
            crate::Mode::EditingUsername => self.show_username_row(),
            crate::Mode::EditingPassword => true
        }
    }

    fn next_allowed_mode(&self, from: crate::Mode) -> crate::Mode {
        let mut cur = from;
        for _ in 0..3 {
            cur = match cur {
                crate::Mode::SelectingSession => crate::Mode::EditingUsername,
                crate::Mode::EditingUsername => crate::Mode::EditingPassword,
                crate::Mode::EditingPassword => crate::Mode::SelectingSession
            };
            if self.mode_allowed(cur) {
                return cur;
            }
        }
        from
    }

    fn prev_allowed_mode(&self, from: crate::Mode) -> crate::Mode {
        let mut cur = from;
        for _ in 0..3 {
            cur = match cur {
                crate::Mode::SelectingSession => crate::Mode::EditingPassword,
                crate::Mode::EditingUsername => crate::Mode::SelectingSession,
                crate::Mode::EditingPassword => crate::Mode::EditingUsername
            };
            if self.mode_allowed(cur) {
                return cur;
            }
        }
        from
    }

    fn goto_next_mode(&mut self) {
        self.mode = self.next_allowed_mode(self.mode);
    }

    fn goto_prev_mode(&mut self) {
        self.mode = self.prev_allowed_mode(self.mode);
    }

    pub(crate) fn greeter_loop(&mut self) {
        let mut username = self
            .forced_username
            .clone()
            .unwrap_or_else(|| String::with_capacity(crate::USERNAME_CAP));
        let mut password = String::with_capacity(crate::PASSWORD_CAP);
        let mut last_username_len = usize::MAX;
        let mut last_password_len = password.len();
        let mut last_target_index = self.target_index;
        let mut last_mode = self.mode;
        let mut needs_redraw = false;

        let mut tty = crate::vt::Tty::new();

        if self.show_target_row() {
            if let Err(e) = self.draw_target() {
                error!("Fatal: unable to draw target session: {e}");
                return;
            }
        }

        if self.show_username_row() {
            if let Err(e) = self.draw_username(&username, true) {
                error!("Fatal: unable to draw username prompt: {e}");
                return;
            }
            last_username_len = username.len();
        }

        loop {
            if self.show_username_row() && username.len() != last_username_len {
                if let Err(e) = self.draw_username(
                    &username,
                    username.len() < last_username_len
                ) {
                    error!("Fatal: unable to draw username prompt: {e}");
                    return;
                }
                last_username_len = username.len();
            }
            if password.len() != last_password_len {
                if let Err(e) = self.draw_password(
                    &password,
                    password.len() < last_password_len
                ) {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                last_password_len = password.len();
            }
            if self.show_target_row() && last_target_index != self.target_index {
                if let Err(e) = self.draw_target() {
                    error!("Fatal: unable to draw target session: {e}");
                    return;
                }
                last_target_index = self.target_index;
            }
            if last_mode != self.mode {
                let bg = self.colors.neutral;
                if let Err(e) = self.draw_bg(&bg) {
                    error!("Fatal: unable to draw background: {e}");
                    return;
                }
                if self.show_target_row() {
                    if let Err(e) = self.draw_target() {
                        error!("Fatal: unable to draw target session: {e}");
                        return;
                    }
                }
                if self.show_username_row() {
                    if let Err(e) = self.draw_username(&username, true) {
                        error!("Fatal: unable to draw username prompt: {e}");
                        return;
                    }
                }
                if let Err(e) = self.draw_password(&password, true) {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                last_mode = self.mode;
            }

            if needs_redraw {
                self.clear();
                let bg = self.colors.neutral;
                if let Err(e) = self.draw_bg(&bg) {
                    error!("Fatal: unable to draw background: {e}");
                    return;
                }
                if self.show_target_row() {
                    if let Err(e) = self.draw_target() {
                        error!("Fatal: unable to draw target session: {e}");
                        return;
                    }
                }
                if self.show_username_row() {
                    if let Err(e) = self.draw_username(&username, true) {
                        error!("Fatal: unable to draw username prompt: {e}");
                        return;
                    }
                }
                if let Err(e) = self.draw_password(&password, true) {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                needs_redraw = false;
                self.refresh();
            }

            let key = match tty.next_event() {
                crate::vt::Event::Key(key) => key,
                crate::vt::Event::Redraw => {
                    needs_redraw = true;
                    continue;
                }
                crate::vt::Event::Eof => {
                    warn!("stdin closed; exiting greeter loop");
                    return;
                }
            };

            match key {
                Key::Ctrl('u') | Key::Ctrl('k') => match self.mode {
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        username.clear();
                    }
                    crate::Mode::EditingPassword => password.clear()
                },
                Key::Ctrl('c') | Key::Ctrl('d') => {
                    username.clear();
                    password.clear();
                    if let Err(e) = self.greetd.cancel() {
                        warn!("Failed to cancel greetd session: {e}");
                    }
                    return;
                }
                Key::Backspace => match self.mode {
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        username.pop();
                    }
                    crate::Mode::EditingPassword => {
                        password.pop();
                    }
                },
                Key::Tab | Key::Down => self.goto_next_mode(),
                Key::Up => self.goto_prev_mode(),
                Key::Enter => match self.mode {
                    crate::Mode::SelectingSession => {
                        self.mode = if self.show_username_row() {
                            crate::Mode::EditingUsername
                        } else {
                            crate::Mode::EditingPassword
                        };
                    }
                    crate::Mode::EditingUsername => {
                        if !username.is_empty() {
                            self.mode = crate::Mode::EditingPassword;
                        }
                    }
                    crate::Mode::EditingPassword => {
                        if password.is_empty() {
                            if self.show_username_row() {
                                username.clear();
                                self.mode = crate::Mode::EditingUsername;
                            }
                        } else {
                            info!(
                                "Attempting login via greetd (session_index={}, username_len={})",
                                self.target_index,
                                username.len()
                            );

                            let username_for_login = if self.show_username_row() {
                                username.clone()
                            } else {
                                self.forced_username
                                    .clone()
                                    .unwrap_or_else(|| username.clone())
                            };
                            let password_for_login =
                                std::mem::take(&mut password);
                            let res = self.greetd.login(
                                username_for_login,
                                password_for_login,
                                self.targets[self.target_index].exec.clone()
                            );

                            password =
                                String::with_capacity(crate::PASSWORD_CAP);
                            match res {
                                Ok(_) => {
                                    info!(
                                        "Login succeeded; exiting greeter loop"
                                    );

                                    if self.blackout_on_success {
                                        let mut buf = crate::buffer::Buffer::new(
                                            self.buf,
                                            self.screen_size
                                        );
                                        buf.memset(&crate::color::Color::from_rgba_u8(
                                            0,
                                            0,
                                            0,
                                            0xFF
                                        ));
                                        self.should_refresh = true;
                                        self.refresh();
                                    }
                                    return;
                                }
                                Err(e) => {
                                    warn!("Login failed: {e}");
                                    let bg = self.colors.error;
                                    if let Err(e) = self.draw_bg(&bg) {
                                        error!("Fatal: unable to draw background: {e}");
                                        return;
                                    }
                                    self.mode = crate::Mode::EditingPassword;
                                    if let Err(e) = self.greetd.cancel() {
                                        warn!("Failed to cancel greetd session after login failure: {e}");
                                    }
                                    needs_redraw = true;
                                }
                            }
                        }
                    }
                },
                Key::Right => {
                    if self.mode == crate::Mode::SelectingSession
                        && self.show_target_row()
                    {
                        self.target_index =
                            (self.target_index + 1) % self.targets.len()
                    }
                }
                Key::Left => {
                    if self.mode == crate::Mode::SelectingSession
                        && self.show_target_row()
                    {
                        if self.target_index == 0 {
                            self.target_index = self.targets.len();
                        }
                        self.target_index -= 1;
                    }
                }
                Key::Char(v) => match self.mode {
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        username.push(v)
                    }
                    crate::Mode::EditingPassword => password.push(v)
                },
                _ => ()
            }
            self.refresh();
        }
    }
}
