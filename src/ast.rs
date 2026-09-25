/// Abstract syntax tree for USGL.

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub eof_comments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub comments: Vec<String>,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expr,
    },
    Expr(Expr),
    Fn {
        name: String,
        params: Vec<Param>,
        ret: Option<Type>,
        body: Vec<Stmt>,
        is_async: bool,
    },
    Return(Option<Expr>),
    If {
        cond: Expr,
        then: Vec<Stmt>,
        alt: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        name: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    Loop {
        body: Vec<Stmt>,
    },
    Match {
        expr: Expr,
        arms: Vec<Arm>,
    },
    Struct {
        name: String,
        fields: Vec<(String, Type)>,
    },
    Enum {
        name: String,
        variants: Vec<(String, Option<Type>)>,
    },
    Import(Vec<String>),
    Test {
        name: String,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Block {
        body: Vec<Stmt>,
        unsafe_block: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pat: Pattern,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wild,
    Lit(LitVal),
    Bind(String),
    Variant {
        ty: Option<String>,
        name: String,
        inner: Option<Box<Pattern>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LitVal {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub is_ref: bool,
    pub is_mut: bool,
    pub is_ptr: bool,
    pub base: String,
    pub generics: Vec<Type>,
}

impl Type {
    pub fn name(n: &str) -> Self {
        Type {
            is_ref: false,
            is_mut: false,
            is_ptr: false,
            base: n.to_string(),
            generics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Ident(String),
    Unary {
        op: char,
        e: Box<Expr>,
    },
    Binary {
        op: BinOp,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        generics: Vec<Type>,
        args: Vec<Arg>,
    },
    Member {
        base: Box<Expr>,
        name: String,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    ErrProp(Box<Expr>),
    Array(Vec<Expr>),
    Map(Vec<(String, Expr)>),
    StructLit {
        ty: String,
        fields: Vec<(String, Expr)>,
    },
    Ctor {
        ty: Option<String>,
        variant: String,
        payload: Option<Box<Expr>>,
    },
    Range(Box<Expr>, Box<Expr>),
    Fn {
        params: Vec<Param>,
        arrow: Option<Box<Expr>>,
        body: Vec<Stmt>,
    },
    If {
        cond: Box<Expr>,
        then: Vec<Stmt>,
        alt: Vec<Stmt>,
    },
    Await(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

impl Arg {
    pub fn pos(value: Expr) -> Self {
        Arg { name: None, value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

/// Walk a program collecting `test "..." { }` blocks.
pub fn collect_tests(program: &Program) -> Vec<(String, Vec<Stmt>, usize)> {
    let mut out = Vec::new();
    for stmt in &program.stmts {
        collect_tests_stmt(stmt, &mut out, 0);
    }
    out
}

fn collect_tests_stmt(stmt: &Stmt, out: &mut Vec<(String, Vec<Stmt>, usize)>, depth: usize) {
    match &stmt.kind {
        StmtKind::Test { name, body } => out.push((name.clone(), body.clone(), depth)),
        StmtKind::If { then, alt, .. } => {
            for s in then {
                collect_tests_stmt(s, out, depth + 1);
            }
            for s in alt {
                collect_tests_stmt(s, out, depth + 1);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Loop { body }
        | StmtKind::Block { body, .. } => {
            for s in body {
                collect_tests_stmt(s, out, depth + 1);
            }
        }
        StmtKind::Fn { body, .. } => {
            for s in body {
                collect_tests_stmt(s, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// Find a top-level `fn main`.
pub fn find_main(program: &Program) -> Option<&Stmt> {
    program
        .stmts
        .iter()
        .find(|s| matches!(&s.kind, StmtKind::Fn { name, .. } if name == "main"))
}
