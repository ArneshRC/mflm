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

    pub fn login(
        &mut self,
        username: String,
        password: String,
        cmd: Vec<String>
    ) -> Result<(), GreetDError> {
        Request::CreateSession { username }
            .write_to(&mut self.stream)
            .map_err(GreetDError::ipc)?;

        Request::PostAuthMessageResponse {
            response: Some(password)
        }
        .write_to(&mut self.stream)
        .map_err(GreetDError::ipc)?;

        let response =
            Response::read_from(&mut self.stream).map_err(GreetDError::ipc)?;
        match response {
            Response::AuthMessage {
                auth_message: _,
                auth_message_type
            } => match auth_message_type {
                AuthMessageType::Secret => {
                    Request::StartSession { cmd }
                        .write_to(&mut self.stream)
                        .map_err(GreetDError::ipc)?;
                    let resp = Response::read_from(&mut self.stream)
                        .map_err(GreetDError::ipc)?;
                    match resp {
                        Response::Success => Ok(()),
                        Response::Error { .. }
                        | Response::AuthMessage { .. } => {
                            Err(GreetDError::AuthFailed(
                                "wrong username or password".to_string()
                            ))
                        }
                    }
                }
                _ => Err(GreetDError::AuthFailed("wrong username".to_string()))
            },
            Response::Success => {
                Request::StartSession { cmd }
                    .write_to(&mut self.stream)
                    .map_err(GreetDError::ipc)?;
                let _ = Response::read_from(&mut self.stream)
                    .map_err(GreetDError::ipc)?;
                Ok(())
            }
            _ => Err(GreetDError::AuthFailed(
                "unknown greetd response".to_string()
            ))
        }
    }

    pub fn cancel(&mut self) -> Result<(), GreetDError> {
        Request::CancelSession
            .write_to(&mut self.stream)
            .map_err(GreetDError::ipc)?;
        let _ =
            Response::read_from(&mut self.stream).map_err(GreetDError::ipc)?;
        Ok(())
    }
}
