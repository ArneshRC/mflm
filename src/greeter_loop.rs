use std::io::Read;

use log::{error, info, warn};

impl<'a> crate::LoginManager<'a> {
    fn mode_allowed(&self, mode: crate::Mode) -> bool {
        match mode {
            crate::Mode::SelectingSession => !self.lock_target,
            crate::Mode::EditingUsername => self.forced_username.is_none(),
            crate::Mode::EditingPassword => true,
        }
    }

    fn next_allowed_mode(&self, from: crate::Mode) -> crate::Mode {
        let mut cur = from;
        for _ in 0..3 {
            cur = match cur {
                crate::Mode::SelectingSession => crate::Mode::EditingUsername,
                crate::Mode::EditingUsername => crate::Mode::EditingPassword,
                crate::Mode::EditingPassword => crate::Mode::SelectingSession,
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
                crate::Mode::EditingPassword => crate::Mode::EditingUsername,
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
        let mut had_failure = false;

        let stdin_handle = std::io::stdin();
        let stdin_lock = stdin_handle.lock();
        let mut stdin_bytes = stdin_lock.bytes();

        let mut read_byte = || -> Option<u8> { stdin_bytes.next().and_then(Result::ok) };

        if !self.lock_target {
            if let Err(e) = self.draw_target() {
                error!("Fatal: unable to draw target session: {e}");
                return;
            }
        }

        loop {
            if self.forced_username.is_none() && username.len() != last_username_len {
                if let Err(e) = self.draw_username(&username, username.len() < last_username_len) {
                    error!("Fatal: unable to draw username prompt: {e}");
                    return;
                }
                last_username_len = username.len();
            }
            if password.len() != last_password_len {
                if let Err(e) = self.draw_password(&password, password.len() < last_password_len) {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                last_password_len = password.len();
            }
            if !self.lock_target && last_target_index != self.target_index {
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
                if !self.lock_target {
                    if let Err(e) = self.draw_target() {
                        error!("Fatal: unable to draw target session: {e}");
                        return;
                    }
                }
                if self.forced_username.is_none() {
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

            if had_failure {
                let bg = self.colors.neutral;
                if let Err(e) = self.draw_bg(&bg) {
                    error!("Fatal: unable to draw background: {e}");
                    return;
                }
                if !self.lock_target {
                    if let Err(e) = self.draw_target() {
                        error!("Fatal: unable to draw target session: {e}");
                        return;
                    }
                }
                if self.forced_username.is_none() {
                    if let Err(e) = self.draw_username(&username, true) {
                        error!("Fatal: unable to draw username prompt: {e}");
                        return;
                    }
                }
                if let Err(e) = self.draw_password(&password, true) {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                had_failure = false;
            }

            let b = match read_byte() {
                Some(b) => b,
                None => {
                    warn!("stdin closed; exiting greeter loop");
                    return;
                }
            };

            match b as char {
                '\x15' | '\x0B' => match self.mode {
                    // ctrl-k/ctrl-u
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.clear();
                        }
                    }
                    crate::Mode::EditingPassword => password.clear(),
                },
                '\x03' | '\x04' => {
                    // ctrl-c/ctrl-D
                    username.clear();
                    password.clear();
                    if let Err(e) = self.greetd.cancel() {
                        warn!("Failed to cancel greetd session: {e}");
                    }
                    return;
                }
                '\x7F' => match self.mode {
                    // backspace
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.pop();
                        }
                    }
                    crate::Mode::EditingPassword => {
                        password.pop();
                    }
                },
                '\t' => self.goto_next_mode(),
                '\r' => match self.mode {
                    crate::Mode::SelectingSession => {
                        self.mode = if self.forced_username.is_some() {
                            crate::Mode::EditingPassword
                        } else {
                            crate::Mode::EditingUsername
                        };
                    }
                    crate::Mode::EditingUsername => {
                        if self.forced_username.is_none() && !username.is_empty() {
                            self.mode = crate::Mode::EditingPassword;
                        }
                    }
                    crate::Mode::EditingPassword => {
                        if password.is_empty() {
                            if self.forced_username.is_none() {
                                username.clear();
                                self.mode = crate::Mode::EditingUsername;
                            }
                        } else {
                            let bg = self.colors.selected;
                            if let Err(e) = self.draw_bg(&bg) {
                                error!("Fatal: unable to draw background: {e}");
                                return;
                            }
                            info!(
                                "Attempting login via greetd (session_index={}, username_len={})",
                                self.target_index,
                                username.len()
                            );

                            let username_for_login = self
                                .forced_username
                                .clone()
                                .unwrap_or_else(|| username.clone());
                            let password_for_login = std::mem::take(&mut password);
                            let res = self.greetd.login(
                                username_for_login,
                                password_for_login,
                                self.targets[self.target_index].exec.clone(),
                            );

                            if self.forced_username.is_none() {
                                username = String::with_capacity(crate::USERNAME_CAP);
                            } else {
                                username = self.forced_username.clone().unwrap();
                            }
                            password = String::with_capacity(crate::PASSWORD_CAP);
                            match res {
                                Ok(_) => {
                                    info!("Login succeeded; exiting greeter loop");
                                    return;
                                }
                                Err(e) => {
                                    warn!("Login failed: {e}");
                                    let bg = self.colors.error;
                                    if let Err(e) = self.draw_bg(&bg) {
                                        error!("Fatal: unable to draw background: {e}");
                                        return;
                                    }
                                    self.mode = if self.forced_username.is_some() {
                                        crate::Mode::EditingPassword
                                    } else {
                                        crate::Mode::EditingUsername
                                    };
                                    if let Err(e) = self.greetd.cancel() {
                                        warn!("Failed to cancel greetd session after login failure: {e}");
                                    }
                                    had_failure = true;
                                }
                            }
                        }
                    }
                },
                // this is terrible
                '\x1b' => match read_byte() {
                    Some(b'[') => match read_byte() {
                        Some(b'A') => self.goto_prev_mode(),
                        Some(b'B') => self.goto_next_mode(),
                        Some(b'C') => match self.mode {
                            crate::Mode::SelectingSession => {
                                if !self.lock_target {
                                    self.target_index = (self.target_index + 1) % self.targets.len()
                                }
                            }
                            _ => (), // TODO: cursor
                        },
                        Some(b'D') => match self.mode {
                            crate::Mode::SelectingSession => {
                                if !self.lock_target {
                                    if self.target_index == 0 {
                                        self.target_index = self.targets.len();
                                    }
                                    self.target_index -= 1;
                                }
                            }
                            _ => (), // TODO: cursor
                        },
                        _ => (), // shrug
                    },
                    _ => (), // shrug
                },
                v => match self.mode {
                    crate::Mode::SelectingSession => (),
                    crate::Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.push(v as char)
                        }
                    }
                    crate::Mode::EditingPassword => password.push(v as char),
                },
            }
            self.refresh();
        }
    }
}
