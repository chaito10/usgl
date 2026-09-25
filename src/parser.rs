use crate::ast::*;
use crate::error::{RtResult, Span, UsglError};
use crate::lexer::Lexer;
use crate::token::{Tok, Token, describe};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pending_comments: Vec<String>,
    /// Set while parsing the subject of a `match` so that the following `{`
    /// starts the arms rather than a struct/map literal.
    suppress_struct_lit: bool,
}

pub fn parse(source: &str, file: &str) -> RtResult<Program> {
    let tokens = Lexer::new(source, file).tokenize()?;
    let mut p = Parser { tokens, pos: 0, pending_comments: Vec::new(), suppress_struct_lit: false };
    p.program()
}

impl Parser {
    // ---------- low-level helpers ----------

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }
    fn peek2(&self) -> &Token {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }
    fn pos_span(&self) -> Span {
        self.peek().span.clone()
    }
    fn next(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn at(&self, k: &Tok) -> bool {
        self.peek().kind == *k
    }
    fn eat(&mut self, k: &Tok) -> bool {
        if self.at(k) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, k: &Tok, msg: &str) -> RtResult<()> {
        if self.at(k) {
            self.next();
            Ok(())
        } else {
            Err(UsglError::parse(format!("{}; found {}", msg, describe(&self.peek().kind)), self.pos_span()))
        }
    }
    fn expect_ident(&mut self, what: &str) -> RtResult<String> {
        if let Tok::Ident(s) = &self.peek().kind {
            let s = s.clone();
            self.next();
            Ok(s)
        } else {
            Err(UsglError::parse(
                format!("expected {}; found {}", what, describe(&self.peek().kind)),
                self.pos_span(),
            ))
        }
    }
    /// Consume a literal `mut` identifier (used for `&mut` in types).
    fn eat_mut(&mut self) -> bool {
        if let Tok::Ident(s) = &self.peek().kind {
            if s == "mut" {
                self.next();
                return true;
            }
        }
        false
    }

    /// Consume newline / comment tokens. Comments are collected into
    /// `pending_comments` so they can be re-emitted by the formatter.
    fn skip_nl(&mut self) {
        loop {
            match &self.peek().kind {
                Tok::Newline => {
                    self.next();
                }
                Tok::Comment(c) => {
                    self.pending_comments.push(c.clone());
                    self.next();
                }
                _ => break,
            }
        }
    }

    /// Like `skip_nl` but reports whether any newline was crossed.
    fn skip_nl_extra(&mut self) -> bool {
        let mut found = false;
        loop {
            match &self.peek().kind {
                Tok::Newline => {
                    found = true;
                    self.next();
                }
                Tok::Comment(c) => {
                    self.pending_comments.push(c.clone());
                    self.next();
                }
                _ => break,
            }
        }
        found
    }

    /// Consume statement separators: newlines and optional `;`.
    fn skip_sep(&mut self) {
        self.skip_nl();
        while self.at(&Tok::Semi) {
            self.next();
            self.skip_nl();
        }
    }

    fn cur_comments(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_comments)
    }

    // ---------- program ----------

    fn program(&mut self) -> RtResult<Program> {
        self.skip_nl();
        let mut stmts = Vec::new();
        loop {
            if self.at(&Tok::Eof) {
                break;
            }
            stmts.push(self.statement()?);
            self.skip_sep();
        }
        let eof_comments = self.cur_comments();
        Ok(Program { stmts, eof_comments })
    }

    fn block_until(&mut self, closing: &Tok) -> RtResult<Vec<Stmt>> {
        let mut body = Vec::new();
        loop {
            self.skip_sep();
            if self.at(closing) {
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(UsglError::parse(
                    format!("expected `{}` before end of file", describe(closing)),
                    self.pos_span(),
                ));
            }
            body.push(self.statement()?);
        }
        Ok(body)
    }

    fn parse_block(&mut self) -> RtResult<Vec<Stmt>> {
        self.skip_nl();
        self.expect(&Tok::LBrace, "expected `{`")?;
        let body = self.block_until(&Tok::RBrace)?;
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(body)
    }

    // ---------- statements ----------

    fn statement(&mut self) -> RtResult<Stmt> {
        self.skip_nl();
        let comments = self.cur_comments();

        // decorators like @no_std / @reflect are parsed and ignored in Phase 1.
        while self.at(&Tok::At) {
            self.next();
            if let Tok::Ident(_) = &self.peek().kind {
                self.next();
            }
            self.skip_nl();
        }

        let kind = if self.at(&Tok::Export) {
            self.next();
            self.inner_statement()?
        } else {
            self.inner_statement()?
        };

        Ok(Stmt { comments, kind })
    }

    fn inner_statement(&mut self) -> RtResult<StmtKind> {
        match &self.peek().kind {
            Tok::Let | Tok::Var | Tok::Const => self.let_stmt(),
            Tok::Fn => self.fn_decl(false),
            Tok::Async => {
                self.next();
                if self.at(&Tok::Fn) {
                    self.fn_decl(true)
                } else {
                    Err(UsglError::parse(
                        "expected `fn` after `async`",
                        self.pos_span(),
                    ))
                }
            }
            Tok::Struct => self.struct_decl(),
            Tok::Enum => self.enum_decl(),
            Tok::Import => self.import_stmt(),
            Tok::Test => self.test_stmt(),
            Tok::Return => {
                self.next();
                if matches!(
                    self.peek().kind,
                    Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof
                ) {
                    Ok(StmtKind::Return(None))
                } else {
                    let e = self.expr()?;
                    Ok(StmtKind::Return(Some(e)))
                }
            }
            Tok::If => self.if_stmt(),
            Tok::While => self.while_stmt(),
            Tok::For => self.for_stmt(),
            Tok::Loop => {
                self.next();
                let body = self.parse_block()?;
                Ok(StmtKind::Loop { body })
            }
            Tok::Match => self.match_stmt(),
            Tok::Break => {
                self.next();
                Ok(StmtKind::Break)
            }
            Tok::Continue => {
                self.next();
                Ok(StmtKind::Continue)
            }
            Tok::Unsafe => {
                self.next();
                let body = self.parse_block()?;
                Ok(StmtKind::Block { body, unsafe_block: true })
            }
            Tok::LBrace => {
                let body = self.parse_block()?;
                Ok(StmtKind::Block { body, unsafe_block: false })
            }
            Tok::Spawn => Err(UsglError::parse(
                "`spawn` is not implemented in the Phase 1 interpreter",
                self.pos_span(),
            )),
            Tok::Trait | Tok::Impl => Err(UsglError::parse(
                "`trait`/`impl` are not implemented in the Phase 1 interpreter",
                self.pos_span(),
            )),
            Tok::Eof => Err(UsglError::parse("unexpected end of file", self.pos_span())),
            _ => {
                let e = self.expr()?;
                if !matches!(
                    self.peek().kind,
                    Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof | Tok::Comma
                ) {
                    return Err(UsglError::parse(
                        format!("expected end of statement; found {}", describe(&self.peek().kind)),
                        self.pos_span(),
                    ));
                }
                Ok(StmtKind::Expr(e))
            }
        }
    }

    fn let_stmt(&mut self) -> RtResult<StmtKind> {
        let kw = self.next().kind;
        let mutable = kw == Tok::Var;
        let name = self.expect_ident("variable name")?;
        let ty = if self.at(&Tok::Colon) {
            self.next();
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.at(&Tok::Eq) {
            self.next();
            self.expr()?
        } else {
            Expr::Int(0)
        };
        Ok(StmtKind::Let { name, mutable, ty, value })
    }

    fn fn_decl(&mut self, is_async: bool) -> RtResult<StmtKind> {
        self.expect(&Tok::Fn, "expected `fn`")?;
        let name = self.expect_ident("function name")?;
        let params = self.parse_params()?;
        let ret = if self.eat(&Tok::RetArrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(StmtKind::Fn { name, params, ret, body, is_async })
    }

    fn parse_params(&mut self) -> RtResult<Vec<Param>> {
        self.expect(&Tok::LParen, "expected `(`")?;
        let mut params = Vec::new();
        self.skip_nl();
        if self.at(&Tok::RParen) {
            self.next();
            return Ok(params);
        }
        loop {
            self.skip_nl();
            let name = self.expect_ident("parameter name")?;
            let ty = if self.at(&Tok::Colon) {
                self.next();
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push(Param { name, ty });
            self.skip_nl();
            if self.eat(&Tok::Comma) {
                continue;
            }
            break;
        }
        self.skip_nl();
        self.expect(&Tok::RParen, "expected `)`")?;
        Ok(params)
    }

    fn struct_decl(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::Struct, "expected `struct`")?;
        let name = self.expect_ident("struct name")?;
        self.skip_nl();
        self.expect(&Tok::LBrace, "expected `{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_nl();
            if self.at(&Tok::RBrace) {
                break;
            }
            let fname = self.expect_ident("field name")?;
            self.expect(&Tok::Colon, "expected `:` after field name")?;
            self.skip_nl();
            let ty = self.parse_type()?;
            fields.push((fname, ty));
            if self.eat(&Tok::Comma) || self.eat(&Tok::Semi) {
                continue;
            }
        }
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(StmtKind::Struct { name, fields })
    }

    fn enum_decl(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::Enum, "expected `enum`")?;
        let name = self.expect_ident("enum name")?;
        self.skip_nl();
        self.expect(&Tok::LBrace, "expected `{`")?;
        let mut variants = Vec::new();
        loop {
            self.skip_nl();
            if self.at(&Tok::RBrace) {
                break;
            }
            let vname = self.expect_ident("variant name")?;
            let payload = if self.at(&Tok::LParen) {
                self.next();
                let ty = self.parse_type()?;
                self.expect(&Tok::RParen, "expected `)`")?;
                Some(ty)
            } else {
                None
            };
            variants.push((vname, payload));
            if self.eat(&Tok::Comma) || self.eat(&Tok::Semi) {
                continue;
            }
        }
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(StmtKind::Enum { name, variants })
    }

    fn import_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::Import, "expected `import`")?;
        self.skip_nl();
        let first = self.expect_ident("module name")?;
        let mut parts = vec![first];
        while self.at(&Tok::Dot) {
            self.next();
            parts.push(self.expect_ident("module path segment")?);
        }
        Ok(StmtKind::Import(parts))
    }

    fn test_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::Test, "expected `test`")?;
        self.skip_nl();
        let name = if let Tok::Str(s) = &self.peek().kind {
            let s = s.clone();
            self.next();
            s
        } else {
            return Err(UsglError::parse("expected test name string", self.pos_span()));
        };
        let body = self.parse_block()?;
        Ok(StmtKind::Test { name, body })
    }

    fn if_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::If, "expected `if`")?;
        let cond = self.expr()?;
        let then = self.parse_block()?;
        let alt = if self.peek_if_else() {
            let mut stmts = Vec::new();
            self.skip_nl();
            self.expect(&Tok::Else, "expected `else`")?;
            if self.at(&Tok::If) {
                let inner = self.if_stmt()?;
                stmts.push(Stmt { comments: Vec::new(), kind: inner });
            } else {
                stmts = self.parse_block()?;
            }
            stmts
        } else {
            Vec::new()
        };
        Ok(StmtKind::If { cond, then, alt })
    }

    /// A `let x = if cond { ... } else { ... }` form. `alt` may hold a nested
    /// `if` statement to represent `else if` chains.
    fn if_expr(&mut self) -> RtResult<Expr> {
        self.expect(&Tok::If, "expected `if`")?;
        self.skip_nl();
        let cond = self.expr()?;
        let then = self.parse_block()?;
        let mut alt = Vec::new();
        if self.peek_if_else() {
            self.skip_nl();
            self.expect(&Tok::Else, "expected `else`")?;
            if self.at(&Tok::If) {
                let inner = self.if_stmt()?;
                alt.push(Stmt { comments: Vec::new(), kind: inner });
            } else {
                alt = self.parse_block()?;
            }
        }
        Ok(Expr::If { cond: Box::new(cond), then, alt })
    }

    /// Look ahead past newlines/comments for an `else`, but never across a
    /// statement boundary that does not end in `else`.
    fn peek_if_else(&mut self) -> bool {
        let mut i = self.pos;
        loop {
            match &self.tokens[i].kind {
                Tok::Newline | Tok::Comment(_) => i += 1,
                Tok::Else => return true,
                _ => return false,
            }
        }
    }

    fn while_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::While, "expected `while`")?;
        let cond = self.expr()?;
        let body = self.parse_block()?;
        Ok(StmtKind::While { cond, body })
    }

    fn for_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::For, "expected `for`")?;
        let name = self.expect_ident("loop variable")?;
        if !(matches!(&self.peek().kind, Tok::Ident(s) if s == "in")) {
            return Err(UsglError::parse("expected `in` in for loop", self.pos_span()));
        }
        self.next();
        let iter = self.expr()?;
        let body = self.parse_block()?;
        Ok(StmtKind::For { name, iter, body })
    }

    fn match_stmt(&mut self) -> RtResult<StmtKind> {
        self.expect(&Tok::Match, "expected `match`")?;
        let saved = self.suppress_struct_lit;
        self.suppress_struct_lit = true;
        let expr = self.expr()?;
        self.suppress_struct_lit = saved;
        self.skip_nl();
        self.expect(&Tok::LBrace, "expected `{` after match expression")?;
        let mut arms = Vec::new();
        loop {
            self.skip_nl();
            if self.at(&Tok::RBrace) {
                break;
            }
            let pat = self.pattern()?;
            self.expect(&Tok::FatArrow, "expected `=>` in match arm")?;
            let body = if self.at(&Tok::LBrace) {
                self.parse_block()?
            } else {
                let e = self.expr()?;
                vec![Stmt { comments: Vec::new(), kind: StmtKind::Expr(e) }]
            };
            arms.push(Arm { pat, body });
            self.skip_nl();
            if self.eat(&Tok::Comma) {
                self.skip_nl();
            } else if self.at(&Tok::RBrace) {
                break;
            }
        }
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(StmtKind::Match { expr, arms })
    }

    fn pattern(&mut self) -> RtResult<Pattern> {
        let span = self.pos_span();
        match &self.peek().kind {
            Tok::Int(n) => {
                let n = *n;
                self.next();
                Ok(Pattern::Lit(LitVal::Int(n)))
            }
            Tok::Float(n) => {
                let n = *n;
                self.next();
                Ok(Pattern::Lit(LitVal::Float(n)))
            }
            Tok::Str(s) => {
                let s = s.clone();
                self.next();
                Ok(Pattern::Lit(LitVal::Str(s)))
            }
            Tok::Char(c) => {
                let c = *c;
                self.next();
                Ok(Pattern::Lit(LitVal::Char(c)))
            }
            Tok::True => {
                self.next();
                Ok(Pattern::Lit(LitVal::Bool(true)))
            }
            Tok::False => {
                self.next();
                Ok(Pattern::Lit(LitVal::Bool(false)))
            }
            Tok::Some => {
                self.next();
                let inner = self.paren_pattern()?;
                Ok(Pattern::Variant { ty: None, name: "Some".into(), inner: Some(Box::new(inner)) })
            }
            Tok::None => {
                self.next();
                Ok(Pattern::Variant { ty: None, name: "None".into(), inner: None })
            }
            Tok::Ok => {
                self.next();
                let inner = if self.at(&Tok::LParen) {
                    let p = self.paren_pattern()?;
                    Some(Box::new(p))
                } else {
                    None
                };
                Ok(Pattern::Variant { ty: None, name: "Ok".into(), inner })
            }
            Tok::Err => {
                self.next();
                let inner = if self.at(&Tok::LParen) {
                    let p = self.paren_pattern()?;
                    Some(Box::new(p))
                } else {
                    None
                };
                Ok(Pattern::Variant { ty: None, name: "Err".into(), inner })
            }
            Tok::Ident(s) => {
                let s = s.clone();
                if s == "_" {
                    self.next();
                    return Ok(Pattern::Wild);
                }
                self.next();
                if self.at(&Tok::Dot) {
                    self.next();
                    let v = self.expect_ident("variant name")?;
                    let inner = if self.at(&Tok::LParen) {
                        let p = self.paren_pattern()?;
                        Some(Box::new(p))
                    } else {
                        None
                    };
                    Ok(Pattern::Variant { ty: Some(s), name: v, inner })
                } else if self.at(&Tok::LParen) {
                    let p = self.paren_pattern()?;
                    Ok(Pattern::Variant { ty: None, name: s, inner: Some(Box::new(p)) })
                } else {
                    Ok(Pattern::Bind(s))
                }
            }
            _ => Err(UsglError::parse(
                format!("invalid match pattern; found {}", describe(&self.peek().kind)),
                span,
            )),
        }
    }

    fn paren_pattern(&mut self) -> RtResult<Pattern> {
        self.expect(&Tok::LParen, "expected `(`")?;
        self.skip_nl();
        let p = self.pattern()?;
        self.skip_nl();
        self.expect(&Tok::RParen, "expected `)`")?;
        Ok(p)
    }

    // ---------- types ----------

    fn parse_type(&mut self) -> RtResult<Type> {
        let is_ref = self.eat(&Tok::Amp);
        let is_mut = if is_ref { self.eat_mut() } else { false };
        let is_ptr = self.eat(&Tok::Star);
        if self.at(&Tok::LBrack) {
            self.next();
            let gen = self.parse_type()?;
            self.expect(&Tok::RBrack, "expected `]` in slice type")?;
            return Ok(Type { is_ref, is_mut, is_ptr, base: "[]".into(), generics: vec![gen] });
        }
        let base = self.expect_ident("type name")?;
        let mut generics = Vec::new();
        if self.at(&Tok::LBrack) {
            self.next();
            loop {
                self.skip_nl();
                generics.push(self.parse_type()?);
                self.skip_nl();
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RBrack, "expected `]` in generic type")?;
        }
        Ok(Type { is_ref, is_mut, is_ptr, base, generics })
    }

    // ---------- expressions ----------

    fn expr(&mut self) -> RtResult<Expr> {
        self.assign()
    }

    fn assign(&mut self) -> RtResult<Expr> {
        let l = self.range_expr()?;
        if self.at(&Tok::Eq) {
            self.next();
            let r = self.assign()?;
            if !is_assignable(&l) {
                return Err(UsglError::parse(
                    "invalid assignment target",
                    self.pos_span(),
                ));
            }
            return Ok(Expr::Assign { target: Box::new(l), value: Box::new(r) });
        }
        Ok(l)
    }

    fn range_expr(&mut self) -> RtResult<Expr> {
        let l = self.pipeline()?;
        if self.at(&Tok::DotDot) {
            self.next();
            let r = self.pipeline()?;
            return Ok(Expr::Range(Box::new(l), Box::new(r)));
        }
        Ok(l)
    }

    fn pipeline(&mut self) -> RtResult<Expr> {
        let mut l = self.logical()?;
        loop {
            if !self.next_significant_is(&Tok::PipeGt) {
                break;
            }
            self.skip_nl();
            self.expect(&Tok::PipeGt, "expected `|>`")?;
            self.skip_nl();
            let rhs = self.postfix()?;
            l = match rhs {
                Expr::Call { callee, generics, mut args } => {
                    args.insert(0, Arg::pos(l));
                    Expr::Call { callee, generics, args }
                }
                Expr::Ident(name) => Expr::Call {
                    callee: Box::new(Expr::Ident(name)),
                    generics: Vec::new(),
                    args: vec![Arg::pos(l)],
                },
                _ => {
                    return Err(UsglError::parse(
                        "right side of `|>` must be a function name or call",
                        self.pos_span(),
                    ))
                }
            };
        }
        Ok(l)
    }

    /// Check (without consuming) whether the next real token is `t`, across
    /// newlines and comments.
    fn next_significant_is(&self, t: &Tok) -> bool {
        let mut i = self.pos;
        loop {
            match &self.tokens[i].kind {
                Tok::Newline | Tok::Comment(_) => i += 1,
                other => return other == t,
            }
        }
    }

    fn logical(&mut self) -> RtResult<Expr> {
        let mut l = self.equality()?;
        loop {
            let op = if self.at(&Tok::AmpAmp) {
                Some(BinOp::And)
            } else if self.at(&Tok::PipePipe) {
                Some(BinOp::Or)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.next();
            let r = self.equality()?;
            l = Expr::Binary { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }

    fn equality(&mut self) -> RtResult<Expr> {
        let mut l = self.relational()?;
        loop {
            let op = if self.at(&Tok::EqEq) {
                Some(BinOp::Eq)
            } else if self.at(&Tok::NotEq) {
                Some(BinOp::Ne)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.next();
            let r = self.relational()?;
            l = Expr::Binary { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }

    fn relational(&mut self) -> RtResult<Expr> {
        let mut l = self.additive()?;
        loop {
            let op = if self.at(&Tok::Lt) {
                Some(BinOp::Lt)
            } else if self.at(&Tok::LtEq) {
                Some(BinOp::Le)
            } else if self.at(&Tok::Gt) {
                Some(BinOp::Gt)
            } else if self.at(&Tok::GtEq) {
                Some(BinOp::Ge)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.next();
            let r = self.additive()?;
            l = Expr::Binary { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }

    fn additive(&mut self) -> RtResult<Expr> {
        let mut l = self.multiplicative()?;
        loop {
            let op = if self.at(&Tok::Plus) {
                Some(BinOp::Add)
            } else if self.at(&Tok::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.next();
            let r = self.multiplicative()?;
            l = Expr::Binary { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }

    fn multiplicative(&mut self) -> RtResult<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = if self.at(&Tok::Star) {
                Some(BinOp::Mul)
            } else if self.at(&Tok::Slash) {
                Some(BinOp::Div)
            } else if self.at(&Tok::Percent) {
                Some(BinOp::Mod)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.next();
            let r = self.unary()?;
            l = Expr::Binary { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }

    fn unary(&mut self) -> RtResult<Expr> {
        if self.at(&Tok::Minus) {
            self.next();
            let e = self.unary()?;
            return Ok(Expr::Unary { op: '-', e: Box::new(e) });
        }
        if self.at(&Tok::Bang) {
            self.next();
            let e = self.unary()?;
            return Ok(Expr::Unary { op: '!', e: Box::new(e) });
        }
        if self.at(&Tok::Await) {
            self.next();
            let e = self.unary()?;
            return Ok(Expr::Await(Box::new(e)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> RtResult<Expr> {
        let mut e = self.operand()?;
        let mut pending_generics: Vec<Type> = Vec::new();
        loop {
            if self.at(&Tok::LBrack) && self.generic_call_ahead() {
                pending_generics = self.parse_generics()?;
                continue;
            }
            if self.at(&Tok::LParen) {
                let args = self.call_args()?;
                let generics = std::mem::take(&mut pending_generics);
                e = Expr::Call { callee: Box::new(e), generics, args };
                continue;
            }
            if self.at(&Tok::Dot) {
                self.next();
                let name = self.expect_ident("member name")?;
                e = Expr::Member { base: Box::new(e), name };
                continue;
            }
            if self.at(&Tok::LBrack) {
                self.next();
                let idx = self.expr()?;
                self.expect(&Tok::RBrack, "expected `]`")?;
                e = Expr::Index { base: Box::new(e), index: Box::new(idx) };
                continue;
            }
            if self.at(&Tok::Question) {
                self.next();
                e = Expr::ErrProp(Box::new(e));
                continue;
            }
            break;
        }
        Ok(e)
    }

    /// Index-like brackets followed by a call are treated as call generics:
    /// `json.parse[Array[User]](data)`.
    fn generic_call_ahead(&self) -> bool {
        let mut i = self.pos;
        if i >= self.tokens.len() || self.tokens[i].kind != Tok::LBrack {
            return false;
        }
        let mut depth = 0usize;
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                Tok::LBrack => depth += 1,
                Tok::RBrack => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if depth != 0 {
            return false;
        }
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                Tok::Newline | Tok::Comment(_) => i += 1,
                Tok::LParen => return true,
                _ => return false,
            }
        }
        false
    }

    fn parse_generics(&mut self) -> RtResult<Vec<Type>> {
        self.expect(&Tok::LBrack, "expected `[`")?;
        let mut out = Vec::new();
        loop {
            self.skip_nl();
            out.push(self.parse_type()?);
            self.skip_nl();
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBrack, "expected `]`")?;
        Ok(out)
    }

    fn call_args(&mut self) -> RtResult<Vec<Arg>> {
        self.expect(&Tok::LParen, "expected `(`")?;
        let mut args = Vec::new();
        self.skip_nl();
        if self.at(&Tok::RParen) {
            self.next();
            return Ok(args);
        }
        loop {
            self.skip_nl();
            // named argument: `name = expr` (but not `==`)
            if let Tok::Ident(n) = &self.peek().kind {
                if matches!(&self.peek2().kind, Tok::Eq) {
                    let name = n.clone();
                    self.next();
                    self.next();
                    let value = self.expr()?;
                    args.push(Arg { name: Some(name), value });
                } else {
                    let value = self.expr()?;
                    args.push(Arg::pos(value));
                }
            } else {
                let value = self.expr()?;
                args.push(Arg::pos(value));
            }
            self.skip_nl();
            if self.eat(&Tok::Comma) {
                continue;
            }
            break;
        }
        self.skip_nl();
        self.expect(&Tok::RParen, "expected `)`")?;
        Ok(args)
    }

    fn operand(&mut self) -> RtResult<Expr> {
        self.skip_nl();
        let span = self.pos_span();
        match &self.peek().kind {
            Tok::If => self.if_expr(),
            Tok::Int(n) => {
                let n = *n;
                self.next();
                Ok(Expr::Int(n))
            }
            Tok::Float(n) => {
                let n = *n;
                self.next();
                Ok(Expr::Float(n))
            }
            Tok::Str(s) => {
                let s = s.clone();
                self.next();
                Ok(Expr::Str(s))
            }
            Tok::Char(c) => {
                let c = *c;
                self.next();
                Ok(Expr::Char(c))
            }
            Tok::True => {
                self.next();
                Ok(Expr::Bool(true))
            }
            Tok::False => {
                self.next();
                Ok(Expr::Bool(false))
            }
            Tok::Ident(name) => {
                let name = name.clone();
                self.next();
                if self.at(&Tok::LBrace) && !self.suppress_struct_lit {
                    let fields = self.struct_literal_fields()?;
                    Ok(Expr::StructLit { ty: name, fields })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Tok::Some => {
                self.next();
                let payload = self.paren_expr()?;
                Ok(Expr::Ctor { ty: None, variant: "Some".into(), payload: Some(Box::new(payload)) })
            }
            Tok::None => {
                self.next();
                Ok(Expr::Ctor { ty: None, variant: "None".into(), payload: None })
            }
            Tok::Ok => {
                self.next();
                let payload = if self.at(&Tok::LParen) { Some(Box::new(self.paren_expr()?)) } else { None };
                Ok(Expr::Ctor { ty: None, variant: "Ok".into(), payload })
            }
            Tok::Err => {
                self.next();
                let payload = if self.at(&Tok::LParen) { Some(Box::new(self.paren_expr()?)) } else { None };
                Ok(Expr::Ctor { ty: None, variant: "Err".into(), payload })
            }
            Tok::Fn => self.fn_expr(),
            Tok::LParen => {
                self.next();
                let e = self.expr()?;
                self.skip_nl();
                self.expect(&Tok::RParen, "expected `)`")?;
                Ok(e)
            }
            Tok::LBrack => {
                self.next();
                let mut items = Vec::new();
                self.skip_nl();
                if self.at(&Tok::RBrack) {
                    self.next();
                    return Ok(Expr::Array(items));
                }
                loop {
                    self.skip_nl();
                    if self.at(&Tok::RBrack) {
                        break;
                    }
                    items.push(self.expr()?);
                    let newline_sep = self.skip_nl_extra();
                    if self.eat(&Tok::Comma) {
                        continue;
                    }
                    if newline_sep && !self.at(&Tok::RBrack) {
                        continue;
                    }
                    break;
                }
                self.skip_nl();
                self.expect(&Tok::RBrack, "expected `]`")?;
                Ok(Expr::Array(items))
            }
            Tok::LBrace => self.map_literal(),
            Tok::Amp => Err(UsglError::parse("borrow `&` is only allowed in type annotations in Phase 1", span)),
            Tok::Pipe | Tok::PipePipe | Tok::AmpAmp | Tok::Eq | Tok::EqEq | Tok::Lt | Tok::LtEq
            | Tok::Gt | Tok::GtEq | Tok::Plus | Tok::Slash | Tok::Star | Tok::Percent
            | Tok::Dot | Tok::DotDot | Tok::Question | Tok::Semi | Tok::Comma | Tok::RBrace
            | Tok::RParen | Tok::RBrack | Tok::Newline | Tok::FatArrow | Tok::RetArrow => Err(
                UsglError::parse(
                    format!("expected an expression; found {}", describe(&self.peek().kind)),
                    span,
                ),
            ),
            _ => Err(UsglError::parse(
                format!("expected an expression; found {}", describe(&self.peek().kind)),
                span,
            )),
        }
    }

    fn paren_expr(&mut self) -> RtResult<Expr> {
        self.expect(&Tok::LParen, "expected `(`")?;
        self.skip_nl();
        let e = self.expr()?;
        self.skip_nl();
        self.expect(&Tok::RParen, "expected `)`")?;
        Ok(e)
    }

    fn fn_expr(&mut self) -> RtResult<Expr> {
        self.expect(&Tok::Fn, "expected `fn`")?;
        let params = self.parse_params()?;
        // Optional return type annotation (type-checked in a later phase).
        if self.at(&Tok::RetArrow) {
            self.next();
            let _ = self.parse_type()?;
            self.skip_nl();
        }
        if self.at(&Tok::FatArrow) {
            self.next();
            self.skip_nl();
            let arrow = self.expr()?;
            Ok(Expr::Fn { params, arrow: Some(Box::new(arrow)), body: Vec::new() })
        } else {
            let body = self.parse_block()?;
            Ok(Expr::Fn { params, arrow: None, body })
        }
    }

    fn struct_literal_fields(&mut self) -> RtResult<Vec<(String, Expr)>> {
        self.expect(&Tok::LBrace, "expected `{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_nl();
            if self.at(&Tok::RBrace) {
                break;
            }
            let name = self.expect_ident("field name")?;
            self.expect(&Tok::Colon, "expected `:` after field name")?;
            self.skip_nl();
            let value = self.expr()?;
            fields.push((name, value));
            self.skip_nl();
            if self.eat(&Tok::Comma) {
                continue;
            }
        }
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(fields)
    }

    fn map_literal(&mut self) -> RtResult<Expr> {
        self.expect(&Tok::LBrace, "expected `{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_nl();
            if self.at(&Tok::RBrace) {
                break;
            }
            let key = match &self.peek().kind {
                Tok::Str(s) => {
                    let s = s.clone();
                    self.next();
                    s
                }
                Tok::Ident(s) => {
                    let s = s.clone();
                    self.next();
                    s
                }
                other => {
                    return Err(UsglError::parse(
                        format!("map key must be a string or identifier; found {}", describe(other)),
                        self.pos_span(),
                    ))
                }
            };
            self.expect(&Tok::Colon, "expected `:` after map key")?;
            self.skip_nl();
            let value = self.expr()?;
            fields.push((key, value));
            self.skip_nl();
            if self.eat(&Tok::Comma) {
                continue;
            }
        }
        self.expect(&Tok::RBrace, "expected `}`")?;
        Ok(Expr::Map(fields))
    }
}

fn is_assignable(e: &Expr) -> bool {
    matches!(e, Expr::Ident(_) | Expr::Member { .. } | Expr::Index { .. })
}