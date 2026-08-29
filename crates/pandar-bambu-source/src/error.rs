use std::{
    cell::RefCell,
    error::Error,
    ffi::{CString, c_char},
    fmt, io,
};

#[derive(Debug)]
pub(crate) enum SessionError {
    Transport {
        operation: &'static str,
        source: io::Error,
    },
    Handshake(io::Error),
    Read {
        operation: &'static str,
        source: io::Error,
    },
    InvalidFrameLength(usize),
    InvalidJpeg,
}

impl SessionError {
    pub(crate) fn transport(operation: &'static str, source: io::Error) -> Self {
        Self::Transport { operation, source }
    }

    pub(crate) fn read(operation: &'static str, source: io::Error) -> Self {
        Self::Read { operation, source }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { operation, .. } => {
                write!(formatter, "local camera transport failed while {operation}")
            }
            Self::Handshake(_) => write!(formatter, "local camera relay handshake failed"),
            Self::Read { operation, .. } => {
                write!(formatter, "local camera stream failed while {operation}")
            }
            Self::InvalidFrameLength(length) => {
                write!(
                    formatter,
                    "local camera relay sent invalid frame length {length}"
                )
            }
            Self::InvalidJpeg => write!(formatter, "local camera relay sent an invalid JPEG frame"),
        }
    }
}

impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport { source, .. } | Self::Read { source, .. } => Some(source),
            Self::Handshake(source) => Some(source),
            Self::InvalidFrameLength(_) | Self::InvalidJpeg => None,
        }
    }
}

pub(crate) enum SessionTerminal {
    Eof,
    Failure(SessionError),
}

pub(crate) fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

pub(crate) fn set_last_error(message: &str) {
    LAST_ERROR.with(|current| {
        *current.borrow_mut() = CString::new(message).expect("session errors contain no NUL bytes");
    });
}

pub(crate) fn last_error_message() -> *const c_char {
    LAST_ERROR.with(|current| current.borrow().as_ptr())
}
