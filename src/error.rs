use std::fmt;

/// A source position. Line and column are 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub file: String,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(file: &str, line: u32, col: u32) -> Self {
        Span {
            file: file.to_string(),
            line,
            col,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// Every error produced by the toolchain.
#[derive(Debug, Clone)]
pub enum UsglError {
    Lex {
        message: String,
        span: Span,
    },
    Parse {
        message: String,
        span: Span,
    },
    Runtime {
        message: String,
        span: Option<Span>,
    },
    /// Static type-checking failure (Phase 2). Spans are not yet attached
    /// to AST nodes, so only the message is carried.
    Type {
        message: String,
    },
    /// Requested process exit (os.exit).
    Exit(i32),
}

impl UsglError {
    pub fn lex(msg: impl Into<String>, span: Span) -> Self {
        UsglError::Lex {
            message: msg.into(),
            span,
        }
    }
    pub fn parse(msg: impl Into<String>, span: Span) -> Self {
        UsglError::Parse {
            message: msg.into(),
            span,
        }
    }
    pub fn rt(msg: impl Into<String>, span: Option<Span>) -> Self {
        UsglError::Runtime {
            message: msg.into(),
            span,
        }
    }
}

impl fmt::Display for UsglError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsglError::Lex { message, span } => write!(f, "lex error at {}: {}", span, message),
            UsglError::Parse { message, span } => write!(f, "parse error at {}: {}", span, message),
            UsglError::Runtime { message, span } => match span {
                Some(s) => write!(f, "error at {}: {}", s, message),
                None => write!(f, "error: {}", message),
            },
            UsglError::Exit(code) => write!(f, "exit({})", code),
            UsglError::Type { message } => write!(f, "type error: {}", message),
        }
    }
}

impl std::error::Error for UsglError {}

/// A helper passed around by the interpreter to construct runtime errors.
pub type RtResult<T> = Result<T, UsglError>;
