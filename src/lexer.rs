use crate::error::{Span, UsglError};
use crate::token::{Tok, Token};

fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "fn" => Tok::Fn,
        "let" => Tok::Let,
        "var" => Tok::Var,
        "const" => Tok::Const,
        "if" => Tok::If,
        "else" => Tok::Else,
        "for" => Tok::For,
        "while" => Tok::While,
        "loop" => Tok::Loop,
        "match" => Tok::Match,
        "struct" => Tok::Struct,
        "enum" => Tok::Enum,
        "import" => Tok::Import,
        "export" => Tok::Export,
        "return" => Tok::Return,
        "async" => Tok::Async,
        "await" => Tok::Await,
        "spawn" => Tok::Spawn,
        "unsafe" => Tok::Unsafe,
        "trait" => Tok::Trait,
        "impl" => Tok::Impl,
        "true" => Tok::True,
        "false" => Tok::False,
        "Some" => Tok::Some,
        "None" => Tok::None,
        "Ok" => Tok::Ok,
        "Err" => Tok::Err,
        "test" => Tok::Test,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        _ => return None,
    })
}

pub struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    file: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, file: &'a str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            file,
        }
    }

    fn cur(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
    fn span(&self) -> Span {
        Span::new(self.file, self.line, self.col)
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.cur()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, UsglError> {
        let mut out = Vec::new();
        let mut pending_nl = false;

        loop {
            let c = match self.cur() {
                Some(c) => c,
                None => {
                    if pending_nl {
                        out.push(Token {
                            kind: Tok::Newline,
                            span: self.span(),
                        });
                    }
                    out.push(Token {
                        kind: Tok::Eof,
                        span: self.span(),
                    });
                    return Ok(out);
                }
            };

            match c {
                ' ' | '\t' | '\r' => {
                    self.bump();
                }
                '\n' => {
                    if !pending_nl {
                        out.push(Token {
                            kind: Tok::Newline,
                            span: self.span(),
                        });
                        pending_nl = true;
                    }
                    self.bump();
                }
                '#' => {
                    self.line_comment();
                }
                '/' if self.peek() == Some('/') => {
                    self.line_comment();
                }
                c if c.is_ascii_digit() => {
                    let tok = self.number()?;
                    out.push(tok);
                    pending_nl = false;
                }
                '"' => {
                    let tok = self.string()?;
                    out.push(tok);
                    pending_nl = false;
                }
                '\'' => {
                    let tok = self.char_lit()?;
                    out.push(tok);
                    pending_nl = false;
                }
                c if c.is_alphabetic() || c == '_' => {
                    let start = self.span();
                    let mut s = String::new();
                    while let Some(ch) = self.cur() {
                        if ch.is_alphanumeric() || ch == '_' {
                            s.push(ch);
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let kind = keyword(&s).unwrap_or(Tok::Ident(s));
                    out.push(Token { kind, span: start });
                    pending_nl = false;
                }
                _ => {
                    let start = self.span();
                    let kind = self.op()?;
                    out.push(Token { kind, span: start });
                    pending_nl = false;
                }
            }
        }
    }

    fn line_comment(&mut self) {
        while let Some(c) = self.cur() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn number(&mut self) -> Result<Token, UsglError> {
        let start = self.span();
        let mut raw = String::new();

        if self.cur() == Some('0') && matches!(self.peek(), Some('x') | Some('X')) {
            self.bump();
            self.bump();
            while let Some(c) = self.cur() {
                if c.is_ascii_hexdigit() {
                    raw.push(c);
                    self.bump();
                } else if c == '_' {
                    self.bump();
                } else {
                    break;
                }
            }
            let v = i64::from_str_radix(&raw, 16)
                .map_err(|_| UsglError::lex("invalid hexadecimal literal", start.clone()))?;
            return Ok(Token {
                kind: Tok::Int(v),
                span: start,
            });
        }

        let mut is_float = false;
        while let Some(c) = self.cur() {
            if c.is_ascii_digit() {
                raw.push(c);
                self.bump();
            } else if c == '_' {
                self.bump();
            } else if c == '.' && self.peek().map_or(false, |n| n.is_ascii_digit()) {
                is_float = true;
                raw.push('.');
                self.bump();
            } else if (c == 'e' || c == 'E')
                && self
                    .peek()
                    .map_or(false, |n| n.is_ascii_digit() || n == '+' || n == '-')
            {
                is_float = true;
                raw.push('e');
                self.bump();
                if matches!(self.cur(), Some('+') | Some('-')) {
                    raw.push(self.cur().unwrap());
                    self.bump();
                }
            } else {
                break;
            }
        }

        if is_float {
            let v = raw
                .parse::<f64>()
                .map_err(|_| UsglError::lex(format!("invalid number `{}`", raw), start.clone()))?;
            Ok(Token {
                kind: Tok::Float(v),
                span: start,
            })
        } else {
            let v = raw.parse::<i64>().map_err(|_| {
                UsglError::lex(format!("integer `{}` out of range", raw), start.clone())
            })?;
            Ok(Token {
                kind: Tok::Int(v),
                span: start,
            })
        }
    }

    fn string(&mut self) -> Result<Token, UsglError> {
        let start = self.span();
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            let c = match self.cur() {
                Some(c) => c,
                None => return Err(UsglError::lex("unterminated string literal", start)),
            };
            self.bump();
            match c {
                '"' => break,
                '\\' => {
                    let e = self.cur().ok_or_else(|| {
                        UsglError::lex("unterminated escape sequence", start.clone())
                    })?;
                    self.bump();
                    match e {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        '0' => s.push('\0'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '\'' => s.push('\''),
                        '\n' => {}
                        other => {
                            return Err(UsglError::lex(
                                format!("unknown escape sequence `\\{}`", other),
                                start,
                            ))
                        }
                    }
                }
                other => s.push(other),
            }
        }
        Ok(Token {
            kind: Tok::Str(s),
            span: start,
        })
    }

    fn char_lit(&mut self) -> Result<Token, UsglError> {
        let start = self.span();
        self.bump(); // opening quote
        let c = match self.cur() {
            Some(c) => c,
            None => return Err(UsglError::lex("unterminated character literal", start)),
        };
        self.bump();
        let c = if c == '\\' {
            let e = self
                .cur()
                .ok_or_else(|| UsglError::lex("unterminated escape sequence", start.clone()))?;
            self.bump();
            match e {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '0' => '\0',
                '\\' => '\\',
                '"' => '"',
                '\'' => '\'',
                other => {
                    return Err(UsglError::lex(
                        format!("unknown escape sequence `\\{}`", other),
                        start,
                    ))
                }
            }
        } else {
            c
        };
        if self.cur() != Some('\'') {
            return Err(UsglError::lex(
                "expected `'` to close character literal",
                start,
            ));
        }
        self.bump();
        Ok(Token {
            kind: Tok::Char(c),
            span: start,
        })
    }

    fn op(&mut self) -> Result<Tok, UsglError> {
        let c = self.cur().unwrap();
        let c2 = self.peek();
        let two: Option<Tok> = match c2 {
            Some('=') if c == '=' => Some(Tok::EqEq),
            Some('>') if c == '=' => Some(Tok::FatArrow),
            Some('=') if c == '!' => Some(Tok::NotEq),
            Some('=') if c == '<' => Some(Tok::LtEq),
            Some('=') if c == '>' => Some(Tok::GtEq),
            Some('&') if c == '&' => Some(Tok::AmpAmp),
            Some('|') if c == '|' => Some(Tok::PipePipe),
            Some('>') if c == '|' => Some(Tok::PipeGt),
            Some('>') if c == '-' => Some(Tok::RetArrow),
            Some('.') if c == '.' => Some(Tok::DotDot),
            _ => None,
        };

        if let Some(t) = two {
            self.bump();
            self.bump();
            return Ok(t);
        }

        self.bump();
        Ok(match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBrack,
            ']' => Tok::RBrack,
            ',' => Tok::Comma,
            ':' => Tok::Colon,
            ';' => Tok::Semi,
            '.' => Tok::Dot,
            '?' => Tok::Question,
            '@' => Tok::At,
            '=' => Tok::Eq,
            '<' => Tok::Lt,
            '>' => Tok::Gt,
            '+' => Tok::Plus,
            '-' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '!' => Tok::Bang,
            '&' => Tok::Amp,
            '|' => Tok::Pipe,
            other => {
                return Err(UsglError::lex(
                    format!("unexpected character `{}`", other),
                    self.span(),
                ))
            }
        })
    }
}
