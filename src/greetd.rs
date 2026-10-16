use std::{env, error::Error, os::unix::net::UnixStream};

use greetd_ipc::{codec::SyncCodec, AuthMessageType, Request, Response};
use thiserror::Error as ThisError;

pub struct GreetD {
    pub stream: UnixStream
}

#[derive(ThisError, Debug)]
#[non_exhaustive]
pub enum GreetDError {
    #[error("GREETD_SOCK environment variable must be defined: {0}")]
    MissingSocketEnv(#[from] env::VarError),

    #[error("failed to connect to greetd socket at {path:?}: {source}")]
    Connect {
        path: String,
        #[source]
        source: std::io::Error
    },

    #[error("greetd IPC error: {0}")]
    Ipc(#[source] Box<dyn Error + Send + Sync>),

    #[error("authentication failed: {0}")]
    AuthFailed(String)
}

impl GreetDError {
    fn ipc<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static
    {
        Self::Ipc(Box::new(err))
    }
}

impl GreetD {
    pub fn new() -> Result<Self, GreetDError> {
        let socket = env::var("GREETD_SOCK")?;
        match UnixStream::connect(&socket) {
            Ok(stream) => Ok(GreetD { stream }),
            Err(source) => Err(GreetDError::Connect {
                path: socket,
                source
            })
        }
    }

    // every request gets exactly one response; reading them in lockstep keeps
    // the stream in sync across failed attempts
    fn request(&mut self, req: Request) -> Result<Response, GreetDError> {
        req.write_to(&mut self.stream).map_err(GreetDError::ipc)?;
        Response::read_from(&mut self.stream).map_err(GreetDError::ipc)
    }

    pub fn login(
        &mut self,
        username: String,
        password: String,
        cmd: Vec<String>
    ) -> Result<(), GreetDError> {
        let mut password = Some(password);
        let mut response = self.request(Request::CreateSession { username })?;
        loop {
            response = match response {
                Response::AuthMessage {
                    auth_message_type, ..
                } => {
                    let reply = match auth_message_type {
                        AuthMessageType::Secret | AuthMessageType::Visible => {
                            match password.take() {
                                Some(p) => Some(p),
                                None => {
                                    return Err(GreetDError::AuthFailed(
                                        "unsupported additional auth prompt"
                                            .to_string()
                                    ))
                                }
                            }
                        }
                        AuthMessageType::Info | AuthMessageType::Error => None
                    };
                    self.request(Request::PostAuthMessageResponse {
                        response: reply
                    })?
                }
                Response::Success => break,
                Response::Error { description, .. } => {
                    return Err(GreetDError::AuthFailed(description))
                }
            };
        }

        match self.request(Request::StartSession { cmd })? {
            Response::Success => Ok(()),
            Response::Error { description, .. } => {
                Err(GreetDError::AuthFailed(description))
            }
            Response::AuthMessage { .. } => Err(GreetDError::AuthFailed(
                "unexpected auth message on session start".to_string()
            ))
        }
    }

    pub fn cancel(&mut self) -> Result<(), GreetDError> {
        self.request(Request::CancelSession)?;
        Ok(())
    }
}
