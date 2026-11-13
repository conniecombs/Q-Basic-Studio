use crate::lexer::Token;
use std::cell::Cell;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    Integer,
    Long,
    Single,
    Double,
    Str,
    FixedStr(usize),
    UserType(String),
    Auto,
}

impl VarType {
    pub fn is_string(&self) -> bool {
        matches!(self, VarType::Str | VarType::FixedStr(_))
    }
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            VarType::Integer | VarType::Long | VarType::Single | VarType::Double
        )
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    StringLit(String),
    Variable(String),
    FieldAccess(Box<Expr>, String),
    ArrayOrCall(String, Vec<Expr>),
    BinaryOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryMinus(Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Xor,
}

impl BinOp {
    fn symbol(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::IntDiv => "\\",
            BinOp::Mod => "MOD",
            BinOp::Pow => "^",
            BinOp::Eq => "=",
            BinOp::NotEq => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "AND",
            BinOp::Or => "OR",
            BinOp::Xor => "XOR",
        }
    }
    fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Debug, Clone)]
pub enum PrintItem {
    Expr(Expr),
    Semicolon,
    Comma,
}

#[derive(Debug, Clone)]
pub enum DataItem {
    Quoted(String),
    Unquoted(String),
}

#[derive(Debug, Clone)]
pub enum LValue {
    Var(String),
    Index(String, Vec<Expr>),
    Field(Box<LValue>, String),
}

impl LValue {
    pub fn root(&self) -> &str {
        match self {
            LValue::Var(n) | LValue::Index(n, _) => n,
            LValue::Field(p, _) => p.root(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub vtype: VarType,
    pub by_ref: bool,
    pub is_array: bool,
}

#[derive(Debug, Clone)]
pub enum CaseClause {
    Value(Expr),
    Range(Expr, Expr),
    Compare(BinOp, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResumeKind {
    Retry,
    Next,
    Label(String),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// Marks the start of a physical source line (1-based) for error reports.
    SrcLine(usize),
    Print {
        handle: Option<Expr>,
        using: Option<Expr>,
        items: Vec<PrintItem>,
    },
    Write {
        handle: Option<Expr>,
        items: Vec<Expr>,
    },
    Let(LValue, Expr),
    MidAssign {
        target: LValue,
        start: Expr,
        len: Option<Expr>,
        value: Expr,
    },
    Input {
        handle: Option<Expr>,
        prompt: Option<String>,
        question: bool,
        vars: Vec<LValue>,
    },
    LineInput {
        handle: Option<Expr>,
        prompt: Option<String>,
        var: LValue,
    },
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        elseifs: Vec<(Expr, Vec<Stmt>)>,
        else_branch: Vec<Stmt>,
    },
    SelectCase {
        expr: Expr,
        cases: Vec<(Vec<CaseClause>, Vec<Stmt>)>,
        else_branch: Vec<Stmt>,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    DoLoop {
        /// (condition, is_until, tested_at_top)
        cond: Option<(Expr, bool, bool)>,
        body: Vec<Stmt>,
    },
    Goto(String),
    Gosub(String),
    Return,
    End,
    Exit(String),
    Label(String),
    LineNumber(u32),
    Dim {
        decls: Vec<DimDecl>,
        shared: bool,
    },
    Redim(Vec<DimDecl>, bool),
    Static(Vec<DimDecl>),
    Shared(Vec<String>),
    Erase(Vec<String>),
    Data(Vec<DataItem>),
    Read(Vec<LValue>),
    Restore(Option<String>),
    Const(String, Expr),
    Noop,
    OptionBase(Expr),
    DefType(VarType, Vec<(char, char)>),
    TypeDef(String, Vec<(String, VarType)>),
    SubDef {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        is_static: bool,
    },
    FuncDef {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        ret_type: VarType,
        is_static: bool,
    },
    CallSub(String, Vec<Expr>),
    Open {
        filename: Expr,
        mode: FileMode,
        handle: Expr,
        record_len: Option<Expr>,
    },
    Close(Vec<Expr>),
    Get {
        handle: Expr,
        record: Option<Expr>,
        var: LValue,
    },
    Put {
        handle: Expr,
        record: Option<Expr>,
        var: LValue,
    },
    Kill(Expr),
    Screen(Expr),
    Window,
    Cls,
    Color(Option<Expr>, Option<Expr>),
    Pset(Expr, Expr, Option<Expr>),
    Line(Option<(Expr, Expr)>, Expr, Expr, Option<Expr>, bool, bool),
    Circle(Expr, Expr, Expr, Option<Expr>),
    Paint(Expr, Expr, Option<Expr>, Option<Expr>),
    Locate(Option<Expr>, Option<Expr>),
    Beep,
    Swap(LValue, LValue),
    Clear,
    Stop,
    System,
    OnJump {
        selector: Expr,
        targets: Vec<String>,
        is_gosub: bool,
    },
    OnError(Option<String>),
    Resume(ResumeKind),
    RaiseError(Expr),
    Randomize(Option<Expr>),
    Sleep(Option<Expr>),
}

#[derive(Debug, Clone)]
pub struct DimDecl {
    pub name: String,
    /// (lower bound, upper bound) per dimension; `None` lower uses OPTION BASE.
    pub dims: Vec<(Option<Expr>, Expr)>,
    pub vtype: VarType,
    /// `true` when declared with `()` but no bounds (e.g. `STATIC A()`).
    pub is_array: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileMode {
    Input,
    Output,
    Append,
    Random,
}

#[derive(Debug)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

type SubSig = (Vec<Param>, bool, VarType);

const MAX_EXPR_DEPTH: usize = 64;

/// Names of built-in functions that take no arguments and may be used
/// without parentheses (`RND`, `TIMER`, `INKEY$` ...).
pub const ZERO_ARG_BUILTINS: &[&str] = &[
    "RND", "TIMER", "INKEY$", "ERR", "ERL", "FREEFILE", "DATE$", "TIME$", "CSRLIN",
];

pub fn parse(tokens: Vec<Token>) -> Result<Program, String> {
    let mut lines = Vec::with_capacity(tokens.len());
    let mut line = 1;
    for t in &tokens {
        lines.push(line);
        if matches!(t, Token::Newline) {
            line += 1;
        }
    }
    let mut p = Parser {
        tokens,
        lines,
        pos: 0,
        cur_line: Cell::new(1),
        var_types: HashMap::new(),
        type_defs: HashMap::new(),
        constants: HashMap::new(),
        sub_sigs: HashMap::new(),
        default_types: Vec::new(),
        depth: 0,
    };
    let stmts = p.parse_program().map_err(|e| {
        if e.starts_with("Line ") {
            e
        } else {
            format!("Line {}: {}", p.cur_line.get(), e)
        }
    })?;
    validate(&stmts, &p.sub_sigs)?;
    Ok(Program { stmts })
}

struct Parser {
    tokens: Vec<Token>,
    lines: Vec<usize>,
    pos: usize,
    cur_line: Cell<usize>,
    var_types: HashMap<String, VarType>,
    type_defs: HashMap<String, Vec<(String, VarType)>>,
    constants: HashMap<String, VarType>,
    sub_sigs: HashMap<String, SubSig>,
    default_types: Vec<(char, char, VarType)>,
    depth: usize,
}

/// Human-readable description of a token for error messages.
fn describe(t: &Token) -> String {
    match t {
        Token::Number(n) => format!("number {}", n),
        Token::StringLit(s) => format!("string \"{}\"", s),
        Token::Identifier(s) => format!("'{}'", s),
        Token::Label(s) => format!("label '{}'", s),
        Token::LineNumber(n) => format!("line number {}", n),
        Token::Newline => "end of line".to_string(),
        Token::Eof => "end of program".to_string(),
        Token::DataItems(_) => "DATA items".to_string(),
        Token::Equals => "'='".to_string(),
        Token::Plus => "'+'".to_string(),
        Token::Minus => "'-'".to_string(),
        Token::Star => "'*'".to_string(),
        Token::Slash => "'/'".to_string(),
        Token::Backslash => "'\\'".to_string(),
        Token::Caret => "'^'".to_string(),
        Token::LParen => "'('".to_string(),
        Token::RParen => "')'".to_string(),
        Token::Comma => "','".to_string(),
        Token::Semicolon => "';'".to_string(),
        Token::Colon => "':'".to_string(),
        Token::Dot => "'.'".to_string(),
        Token::Hash => "'#'".to_string(),
        Token::NotEqual => "'<>'".to_string(),
        Token::LessEqual => "'<='".to_string(),
        Token::GreaterEqual => "'>='".to_string(),
        Token::Less => "'<'".to_string(),
        Token::Greater => "'>'".to_string(),
        Token::EndIf => "END IF".to_string(),
        Token::EndSub => "END SUB".to_string(),
        Token::EndFunction => "END FUNCTION".to_string(),
        Token::EndType => "END TYPE".to_string(),
        Token::EndSelect => "END SELECT".to_string(),
        Token::StringKw => "STRING".to_string(),
        Token::LenKw => "LEN".to_string(),
        other => format!("{:?}", other).to_uppercase(),
    }
}

fn is_block_terminator(t: &Token) -> bool {
    matches!(
        t,
        Token::Next
            | Token::Wend
            | Token::Loop
            | Token::EndIf
            | Token::Else
            | Token::ElseIf
            | Token::Case
            | Token::EndSelect
            | Token::EndSub
            | Token::EndFunction
            | Token::EndType
    )
}

impl Parser {
    fn tok(&self, i: usize) -> &Token {
        self.tokens.get(i).unwrap_or(&Token::Eof)
    }
    fn peek(&self) -> &Token {
        if let Some(l) = self.lines.get(self.pos) {
            self.cur_line.set(*l);
        }
        self.tok(self.pos)
    }
    fn peek_at(&self, off: usize) -> &Token {
        self.tok(self.pos + off)
    }
    fn line_here(&self) -> usize {
        self.lines
            .get(self.pos)
            .copied()
            .unwrap_or(self.cur_line.get())
    }
    fn advance(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos < self.tokens.len().saturating_sub(1) {
            self.pos += 1;
        }
        t
    }
    fn check(&self, t: &Token) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(t)
    }
    fn matches(&mut self, t: &Token) -> bool {
        if self.check(t) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Token, what: &str) -> Result<(), String> {
        if self.matches(t) {
            Ok(())
        } else {
            Err(format!(
                "Expected {} but found {}",
                what,
                describe(self.peek())
            ))
        }
    }
    fn at_stmt_end(&self) -> bool {
        matches!(
            self.peek(),
            Token::Newline | Token::Eof | Token::Colon | Token::Else
        )
    }
    fn is_line_marker(token: &Token) -> bool {
        matches!(token, Token::LineNumber(_) | Token::Label(_))
    }
    fn at_block_boundary(&self, boundaries: &[Token]) -> bool {
        let mut offset = 0;
        while Self::is_line_marker(self.peek_at(offset)) {
            offset += 1;
        }
        let token = self.peek_at(offset);
        boundaries
            .iter()
            .any(|b| std::mem::discriminant(token) == std::mem::discriminant(b))
    }
    fn consume_line_markers(&mut self) {
        while Self::is_line_marker(self.peek()) {
            self.advance();
        }
    }
    fn consume_to_line_end(&mut self) {
        while !matches!(
            self.peek(),
            Token::Colon | Token::Newline | Token::Eof | Token::Else
        ) {
            self.advance();
        }
    }
    fn skip_separators(&mut self) {
        while matches!(self.peek(), Token::Newline | Token::Colon) {
            self.advance();
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        self.prescan_signatures();
        let mut stmts = Vec::new();
        loop {
            self.skip_separators();
            if matches!(self.peek(), Token::Eof) {
                break;
            }
            stmts.extend(self.parse_line()?);
        }
        Ok(stmts)
    }

    /// Parse statements until one of `ends` starts a statement. Line markers in
    /// front of the terminator are kept so jumps to that line still work.
    fn parse_block(&mut self, ends: &[Token]) -> Result<Vec<Stmt>, String> {
        let mut body = Vec::new();
        loop {
            self.skip_separators();
            if matches!(self.peek(), Token::Eof) {
                return Ok(body);
            }
            if self.at_block_boundary(ends) {
                while Self::is_line_marker(self.peek()) {
                    match self.advance() {
                        Token::LineNumber(n) => body.push(Stmt::LineNumber(n)),
                        Token::Label(s) => body.push(Stmt::Label(s)),
                        _ => {}
                    }
                }
                return Ok(body);
            }
            body.extend(self.parse_line()?);
        }
    }

    fn prescan_signatures(&mut self) {
        let mut i = 0;
        while i < self.tokens.len() {
            let is_func = match self.tok(i) {
                Token::Sub => false,
                Token::Function => true,
                _ => {
                    i += 1;
                    continue;
                }
            };
            // Skip `DECLARE SUB`, `EXIT SUB` and `END SUB` style uses.
            if i > 0 && matches!(self.tok(i - 1), Token::Exit | Token::Declare) {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            let name = match self.tok(j) {
                Token::Identifier(n) => n.clone(),
                _ => {
                    i += 1;
                    continue;
                }
            };
            j += 1;
            let mut params: Vec<Param> = Vec::new();
            if matches!(self.tok(j), Token::LParen) {
                j += 1;
                while !matches!(self.tok(j), Token::RParen | Token::Eof | Token::Newline) {
                    let by_ref = match self.tok(j) {
                        Token::ByVal => {
                            j += 1;
                            false
                        }
                        Token::ByRef => {
                            j += 1;
                            true
                        }
                        _ => true,
                    };
                    let pname = match self.tok(j) {
                        Token::Identifier(n) => n.clone(),
                        _ => break,
                    };
                    j += 1;
                    let mut is_array = false;
                    if matches!(self.tok(j), Token::LParen)
                        && matches!(self.tok(j + 1), Token::RParen)
                    {
                        j += 2;
                        is_array = true;
                    }
                    let vtype = if matches!(self.tok(j), Token::As) {
                        j += 1;
                        let t = token_type(self.tok(j)).unwrap_or(VarType::Auto);
                        j += 1;
                        t
                    } else {
                        self.infer_type_for_name(&pname)
                    };
                    params.push(Param {
                        name: pname,
                        vtype,
                        by_ref,
                        is_array,
                    });
                    if matches!(self.tok(j), Token::Comma) {
                        j += 1;
                    }
                }
                if matches!(self.tok(j), Token::RParen) {
                    j += 1;
                }
            }
            let ret_type = if is_func {
                if matches!(self.tok(j), Token::As) {
                    token_type(self.tok(j + 1)).unwrap_or_else(|| infer_type_from_suffix(&name))
                } else {
                    infer_type_from_suffix(&name)
                }
            } else {
                VarType::Auto
            };
            self.sub_sigs.insert(name, (params, is_func, ret_type));
            i = j;
        }
    }

    fn parse_line(&mut self) -> Result<Vec<Stmt>, String> {
        let mut result = Vec::new();
        if let Token::LineNumber(n) = self.peek().clone() {
            self.advance();
            result.push(Stmt::LineNumber(n));
        }
        if let Token::Label(name) = self.peek().clone() {
            self.advance();
            result.push(Stmt::Label(name));
        }
        result.push(Stmt::SrcLine(self.line_here()));
        loop {
            if matches!(self.peek(), Token::Newline | Token::Eof) {
                break;
            }
            if matches!(self.peek(), Token::Colon) {
                self.advance();
                continue;
            }
            let s = self.parse_stmt()?;
            result.push(s);
            if matches!(self.peek(), Token::Colon) {
                self.advance();
                if is_block_terminator(self.peek()) {
                    return Ok(result);
                }
                continue;
            }
            if matches!(self.peek(), Token::Newline | Token::Eof) {
                break;
            }
            if is_block_terminator(self.peek()) {
                return Ok(result);
            }
            return Err(format!(
                "Expected end of statement but found {}",
                describe(self.peek())
            ));
        }
        if matches!(self.peek(), Token::Newline) {
            self.advance();
        }
        Ok(result)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek().clone() {
            Token::Identifier(_) => self.parse_id_stmt(),
            Token::Print => self.parse_print(),
            Token::Write => self.parse_write(),
            Token::Let => {
                self.advance();
                self.parse_assignment()
            }
            Token::Input => self.parse_input(),
            Token::If => self.parse_if(),
            Token::Select => self.parse_select(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Do => self.parse_do(),
            Token::Goto => {
                self.advance();
                self.parse_goto_target().map(Stmt::Goto)
            }
            Token::Gosub => {
                self.advance();
                self.parse_goto_target().map(Stmt::Gosub)
            }
            Token::Return => {
                self.advance();
                Ok(Stmt::Return)
            }
            Token::End => {
                self.advance();
                Ok(Stmt::End)
            }
            Token::Exit => self.parse_exit(),
            Token::Dim => self.parse_dim(),
            Token::Redim => self.parse_redim(),
            Token::Static => {
                self.advance();
                let decls = self.parse_dim_decls()?;
                Ok(Stmt::Static(decls))
            }
            Token::Shared => self.parse_shared(),
            Token::Erase => self.parse_erase(),
            Token::Data => self.parse_data(),
            Token::Read => self.parse_read(),
            Token::Restore => self.parse_restore(),
            Token::Const => self.parse_const(),
            Token::Declare => self.parse_declare(),
            Token::Option => self.parse_option(),
            Token::DefInt | Token::DefLng | Token::DefSng | Token::DefDbl | Token::DefStr => {
                self.parse_def_type()
            }
            Token::Type => self.parse_type_def(),
            Token::Sub => self.parse_sub(),
            Token::Function => self.parse_function(),
            Token::Call => self.parse_call(),
            Token::On => self.parse_on(),
            Token::Error => {
                self.advance();
                Ok(Stmt::RaiseError(self.parse_expr()?))
            }
            Token::Resume => self.parse_resume(),
            Token::Open => self.parse_open(),
            Token::Close => self.parse_close(),
            Token::Get => self.parse_get_put(true),
            Token::Put => self.parse_get_put(false),
            Token::Kill => {
                self.advance();
                Ok(Stmt::Kill(self.parse_expr()?))
            }
            Token::Screen => {
                self.advance();
                let e = self.parse_expr()?;
                // Ignore the optional colour-switch / page arguments.
                self.consume_to_line_end();
                Ok(Stmt::Screen(e))
            }
            Token::Window => {
                self.advance();
                self.consume_to_line_end();
                Ok(Stmt::Window)
            }
            Token::Cls => {
                self.advance();
                if !self.at_stmt_end() {
                    let _ = self.parse_expr()?;
                }
                Ok(Stmt::Cls)
            }
            Token::Color => self.parse_color(),
            Token::Pset => self.parse_pset(),
            Token::Line if matches!(self.peek_at(1), Token::Input) => self.parse_line_input(),
            Token::Line => self.parse_line_stmt(),
            Token::Circle => self.parse_circle(),
            Token::Paint => self.parse_paint(),
            Token::Locate => self.parse_locate(),
            Token::Beep => {
                self.advance();
                Ok(Stmt::Beep)
            }
            Token::Swap => self.parse_swap(),
            Token::Clear => {
                self.advance();
                self.consume_to_line_end();
                Ok(Stmt::Clear)
            }
            Token::Stop => {
                self.advance();
                Ok(Stmt::Stop)
            }
            Token::System => {
                self.advance();
                Ok(Stmt::System)
            }
            Token::Randomize => self.parse_randomize(),
            Token::Sleep => {
                self.advance();
                let arg = if self.at_stmt_end() {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(Stmt::Sleep(arg))
            }
            Token::Next => Err("NEXT without FOR".to_string()),
            Token::Wend => Err("WEND without WHILE".to_string()),
            Token::Loop => Err("LOOP without DO".to_string()),
            Token::EndIf => Err("END IF without block IF".to_string()),
            Token::Else => Err("ELSE without IF".to_string()),
            Token::ElseIf => Err("ELSEIF without block IF".to_string()),
            Token::Case => Err("CASE without SELECT CASE".to_string()),
            Token::EndSelect => Err("END SELECT without SELECT CASE".to_string()),
            Token::EndSub => Err("END SUB without SUB".to_string()),
            Token::EndFunction => Err("END FUNCTION without FUNCTION".to_string()),
            Token::EndType => Err("END TYPE without TYPE".to_string()),
            other => Err(format!(
                "Expected a statement but found {}",
                describe(&other)
            )),
        }
    }

    fn parse_goto_target(&mut self) -> Result<String, String> {
        match self.advance() {
            Token::Number(n) if n >= 0.0 && n.fract() == 0.0 => Ok(format!("{}", n as u32)),
            Token::Identifier(s) => Ok(s),
            t => Err(format!(
                "Expected a line number or label but found {}",
                describe(&t)
            )),
        }
    }

    /// Parses an optional `#n,` file handle prefix.
    fn parse_handle_prefix(&mut self) -> Result<Option<Expr>, String> {
        if !matches!(self.peek(), Token::Hash) {
            return Ok(None);
        }
        self.advance();
        let h = self.parse_expr()?;
        if !self.at_stmt_end() {
            self.expect(&Token::Comma, "',' after file number")?;
        }
        Ok(Some(h))
    }

    fn parse_print(&mut self) -> Result<Stmt, String> {
        self.advance();
        let handle = self.parse_handle_prefix()?;
        let using = if self.matches(&Token::Using) {
            let fmt = self.parse_expr()?;
            if !self.matches(&Token::Semicolon) {
                self.expect(&Token::Comma, "';' after PRINT USING format")?;
            }
            Some(fmt)
        } else {
            None
        };
        let mut items = Vec::new();
        loop {
            match self.peek() {
                Token::Newline
                | Token::Eof
                | Token::Colon
                | Token::Else
                | Token::ElseIf
                | Token::EndIf => break,
                Token::Semicolon => {
                    self.advance();
                    items.push(PrintItem::Semicolon);
                }
                Token::Comma => {
                    self.advance();
                    items.push(PrintItem::Comma);
                }
                _ => {
                    let e = self.parse_expr()?;
                    items.push(PrintItem::Expr(e));
                }
            }
        }
        Ok(Stmt::Print {
            handle,
            using,
            items,
        })
    }

    fn parse_write(&mut self) -> Result<Stmt, String> {
        self.advance();
        let handle = self.parse_handle_prefix()?;
        let mut items = Vec::new();
        while !self.at_stmt_end() {
            items.push(self.parse_expr()?);
            if !self.matches(&Token::Comma) && !self.matches(&Token::Semicolon) {
                break;
            }
        }
        Ok(Stmt::Write { handle, items })
    }

    fn parse_id_stmt(&mut self) -> Result<Stmt, String> {
        let start = self.pos;
        let name = match self.peek().clone() {
            Token::Identifier(n) => n,
            _ => unreachable!(),
        };
        if name == "DEF" && matches!(self.peek_at(1), Token::Identifier(_)) {
            return self.parse_def_fn();
        }
        if name == "MID$" && matches!(self.peek_at(1), Token::LParen) {
            return self.parse_mid_assign();
        }
        if let Ok(lv) = self.parse_lvalue() {
            if self.matches(&Token::Equals) {
                return self.finish_assignment(lv);
            }
        }
        self.pos = start;
        self.advance();
        let mut args = Vec::new();
        if matches!(self.peek(), Token::LParen) && !self.sub_has_no_params(&name) {
            self.advance();
            if !matches!(self.peek(), Token::RParen) {
                args = self.parse_expr_list()?;
            }
            self.expect(&Token::RParen, "')'")?;
            return Ok(Stmt::CallSub(name, args));
        }
        if matches!(self.peek(), Token::LParen) {
            // `DEMO()` for a SUB without parameters.
            self.advance();
            self.expect(&Token::RParen, "')'")?;
            return Ok(Stmt::CallSub(name, args));
        }
        if !self.at_stmt_end() {
            args = self.parse_expr_list()?;
        }
        Ok(Stmt::CallSub(name, args))
    }

    fn sub_has_no_params(&self, name: &str) -> bool {
        self.sub_sigs
            .get(name)
            .map(|(p, _, _)| p.is_empty())
            .unwrap_or(false)
    }

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = vec![self.parse_expr()?];
        while self.matches(&Token::Comma) {
            args.push(self.parse_expr()?);
        }
        Ok(args)
    }

    fn parse_def_fn(&mut self) -> Result<Stmt, String> {
        self.advance(); // DEF
        let name = match self.advance() {
            Token::Identifier(n) if n.starts_with("FN") => n,
            t => {
                return Err(format!(
                    "Expected a function name starting with FN but found {}",
                    describe(&t)
                ))
            }
        };
        let mut params = self.parse_params()?;
        for p in &mut params {
            p.by_ref = false;
        }
        if !self.matches(&Token::Equals) {
            return Err("Multi-line DEF FN is not supported; use FUNCTION instead".to_string());
        }
        let saved_types = self.var_types.clone();
        for p in &params {
            self.var_types.insert(p.name.clone(), p.vtype.clone());
        }
        let line = self.line_here();
        let expr = self.parse_expr()?;
        let ret_type = self.infer_type_for_name(&name);
        self.var_types.insert(name.clone(), ret_type.clone());
        self.check_assignment_types(&LValue::Var(name.clone()), &expr)?;
        self.var_types = saved_types;
        self.sub_sigs
            .insert(name.clone(), (params.clone(), true, ret_type.clone()));
        Ok(Stmt::FuncDef {
            body: vec![
                Stmt::SrcLine(line),
                Stmt::Let(LValue::Var(name.clone()), expr),
            ],
            name,
            params,
            ret_type,
            is_static: false,
        })
    }

    fn parse_mid_assign(&mut self) -> Result<Stmt, String> {
        self.advance(); // MID$
        self.advance(); // (
        let target = self.parse_lvalue()?;
        if !self.lvalue_type(&target).is_string() {
            return Err("MID$ statement requires a string variable".to_string());
        }
        self.expect(&Token::Comma, "','")?;
        let start = self.parse_expr()?;
        let len = if self.matches(&Token::Comma) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&Token::RParen, "')'")?;
        self.expect(&Token::Equals, "'='")?;
        let value = self.parse_expr()?;
        if !self.expr_type(&value).is_string() && self.expr_type(&value) != VarType::Auto {
            return Err("Type mismatch: MID$ statement needs a string value".to_string());
        }
        Ok(Stmt::MidAssign {
            target,
            start,
            len,
            value,
        })
    }

    fn parse_assignment(&mut self) -> Result<Stmt, String> {
        let lv = self.parse_lvalue()?;
        self.expect(&Token::Equals, "'=' in assignment")?;
        self.finish_assignment(lv)
    }

    fn finish_assignment(&mut self, lv: LValue) -> Result<Stmt, String> {
        let e = self.parse_expr()?;
        self.declare_lvalue_if_needed(&lv);
        self.check_assignment_types(&lv, &e)?;
        Ok(Stmt::Let(lv, e))
    }

    fn parse_input(&mut self) -> Result<Stmt, String> {
        self.advance();
        let handle = self.parse_handle_prefix()?;
        // `INPUT ;` keeps the cursor on the same line; accepted and ignored.
        self.matches(&Token::Semicolon);
        let mut prompt = None;
        let mut question = true;
        if handle.is_none() {
            if let Token::StringLit(s) = self.peek().clone() {
                self.advance();
                prompt = Some(s);
                if self.matches(&Token::Comma) {
                    question = false;
                } else {
                    self.expect(&Token::Semicolon, "';' or ',' after INPUT prompt")?;
                }
            }
        }
        let mut vars = Vec::new();
        loop {
            let lv = self.parse_lvalue()?;
            self.declare_lvalue_if_needed(&lv);
            vars.push(lv);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::Input {
            handle,
            prompt,
            question,
            vars,
        })
    }

    fn parse_line_input(&mut self) -> Result<Stmt, String> {
        self.advance();
        self.advance();
        let handle = self.parse_handle_prefix()?;
        self.matches(&Token::Semicolon);
        let mut prompt = None;
        if handle.is_none() {
            if let Token::StringLit(s) = self.peek().clone() {
                self.advance();
                prompt = Some(s);
                if !self.matches(&Token::Semicolon) {
                    self.expect(&Token::Comma, "';' after LINE INPUT prompt")?;
                }
            }
        }
        let var = self.parse_lvalue()?;
        self.declare_lvalue_if_needed(&var);
        if !self.lvalue_type(&var).is_string() {
            return Err("LINE INPUT requires a string variable".to_string());
        }
        Ok(Stmt::LineInput {
            handle,
            prompt,
            var,
        })
    }

    fn parse_inline_stmts(&mut self) -> Result<Vec<Stmt>, String> {
        if let Token::Number(n) = self.peek().clone() {
            self.advance();
            return Ok(vec![Stmt::Goto(format!("{}", n as u32))]);
        }
        let mut stmts = vec![self.parse_stmt()?];
        while matches!(self.peek(), Token::Colon) {
            self.advance();
            if matches!(self.peek(), Token::Else | Token::Newline | Token::Eof) {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let cond = self.parse_expr()?;
        if !self.matches(&Token::Then) {
            // `IF X GOTO 100` form.
            if matches!(self.peek(), Token::Goto) {
                self.advance();
                let target = self.parse_goto_target()?;
                return Ok(Stmt::If {
                    cond,
                    then_branch: vec![Stmt::Goto(target)],
                    elseifs: Vec::new(),
                    else_branch: Vec::new(),
                });
            }
            return Err(format!(
                "Expected THEN after IF condition but found {}",
                describe(self.peek())
            ));
        }
        let mut elseifs: Vec<(Expr, Vec<Stmt>)> = Vec::new();
        let mut else_branch = Vec::new();
        if matches!(self.peek(), Token::Newline | Token::Eof) {
            let ends = [Token::Else, Token::ElseIf, Token::EndIf];
            let then_branch = self.parse_block(&ends)?;
            while matches!(self.peek(), Token::ElseIf) {
                self.advance();
                let c = self.parse_expr()?;
                self.expect(&Token::Then, "THEN after ELSEIF condition")?;
                let block = self.parse_block(&ends)?;
                elseifs.push((c, block));
            }
            if self.matches(&Token::Else) {
                else_branch = self.parse_block(&[Token::EndIf])?;
            }
            if !self.matches(&Token::EndIf) {
                return Err(format!("Line {}: Block IF without END IF", start_line));
            }
            return Ok(Stmt::If {
                cond,
                then_branch,
                elseifs,
                else_branch,
            });
        }
        let then_branch = self.parse_inline_stmts()?;
        if self.matches(&Token::Else) {
            else_branch = self.parse_inline_stmts()?;
        }
        Ok(Stmt::If {
            cond,
            then_branch,
            elseifs,
            else_branch,
        })
    }

    fn parse_select(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        self.expect(&Token::Case, "CASE after SELECT")?;
        let expr = self.parse_expr()?;
        let mut cases = Vec::new();
        let mut else_branch = Vec::new();
        let ends = [Token::Case, Token::EndSelect];
        loop {
            self.skip_separators();
            if self.at_block_boundary(&ends) {
                self.consume_line_markers();
            }
            if !matches!(self.peek(), Token::Case) {
                break;
            }
            self.advance();
            if self.matches(&Token::Else) {
                else_branch = self.parse_block(&ends)?;
                continue;
            }
            let mut clauses = Vec::new();
            loop {
                let clause = if self.matches(&Token::Is) {
                    let op = match self.advance() {
                        Token::Equals => BinOp::Eq,
                        Token::NotEqual => BinOp::NotEq,
                        Token::Less => BinOp::Lt,
                        Token::LessEqual => BinOp::Le,
                        Token::Greater => BinOp::Gt,
                        Token::GreaterEqual => BinOp::Ge,
                        t => {
                            return Err(format!(
                                "Expected a comparison after CASE IS but found {}",
                                describe(&t)
                            ))
                        }
                    };
                    CaseClause::Compare(op, self.parse_expr()?)
                } else {
                    let a = self.parse_expr()?;
                    if self.matches(&Token::To) {
                        CaseClause::Range(a, self.parse_expr()?)
                    } else {
                        CaseClause::Value(a)
                    }
                };
                clauses.push(clause);
                if !self.matches(&Token::Comma) {
                    break;
                }
            }
            let body = self.parse_block(&ends)?;
            cases.push((clauses, body));
        }
        if !self.matches(&Token::EndSelect) {
            if matches!(self.peek(), Token::Eof) {
                return Err(format!(
                    "Line {}: SELECT CASE without END SELECT",
                    start_line
                ));
            }
            return Err(format!(
                "Expected CASE or END SELECT but found {}",
                describe(self.peek())
            ));
        }
        Ok(Stmt::SelectCase {
            expr,
            cases,
            else_branch,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let var = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a variable after FOR but found {}",
                    describe(&t)
                ))
            }
        };
        self.declare_var_if_needed(&var);
        if !self.lookup_var_type(&var).is_numeric() && self.lookup_var_type(&var) != VarType::Auto {
            return Err(format!("FOR variable '{}' must be numeric", var));
        }
        self.expect(&Token::Equals, "'=' in FOR")?;
        let start = self.parse_expr()?;
        self.expect(&Token::To, "TO in FOR")?;
        let end = self.parse_expr()?;
        let step = if self.matches(&Token::Step) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_numeric(&start, "FOR start")?;
        self.expect_numeric(&end, "FOR end")?;
        if let Some(s) = &step {
            self.expect_numeric(s, "FOR step")?;
        }
        let body = self.parse_block(&[Token::Next])?;
        if !self.matches(&Token::Next) {
            return Err(format!("Line {}: FOR without NEXT", start_line));
        }
        if let Token::Identifier(n) = self.peek().clone() {
            if n != var {
                return Err(format!("NEXT {} does not match FOR {}", n, var));
            }
            self.advance();
            // `NEXT J, I` closes several loops: leave a synthetic NEXT behind.
            if matches!(self.peek(), Token::Comma) {
                self.tokens[self.pos] = Token::Next;
            }
        }
        Ok(Stmt::For {
            var,
            start,
            end,
            step,
            body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let cond = self.parse_expr()?;
        let body = self.parse_block(&[Token::Wend])?;
        if !self.matches(&Token::Wend) {
            return Err(format!("Line {}: WHILE without WEND", start_line));
        }
        Ok(Stmt::While { cond, body })
    }

    fn parse_do(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let mut top_cond = None;
        if self.matches(&Token::While) {
            top_cond = Some((self.parse_expr()?, false, true));
        } else if self.matches(&Token::Until) {
            top_cond = Some((self.parse_expr()?, true, true));
        }
        let body = self.parse_block(&[Token::Loop])?;
        if !self.matches(&Token::Loop) {
            return Err(format!("Line {}: DO without LOOP", start_line));
        }
        let cond = if top_cond.is_some() {
            top_cond
        } else if self.matches(&Token::While) {
            Some((self.parse_expr()?, false, false))
        } else if self.matches(&Token::Until) {
            Some((self.parse_expr()?, true, false))
        } else {
            None
        };
        Ok(Stmt::DoLoop { cond, body })
    }

    fn parse_exit(&mut self) -> Result<Stmt, String> {
        self.advance();
        let kind = match self.advance() {
            Token::For => "FOR",
            Token::Sub => "SUB",
            Token::Function => "FUNCTION",
            Token::Do => "DO",
            Token::While => "WHILE",
            Token::Identifier(s) if s == "DEF" => "FUNCTION",
            t => {
                return Err(format!(
                    "Expected FOR, DO, WHILE, SUB or FUNCTION after EXIT but found {}",
                    describe(&t)
                ))
            }
        };
        Ok(Stmt::Exit(kind.to_string()))
    }

    fn parse_dim(&mut self) -> Result<Stmt, String> {
        self.advance();
        let shared = self.matches(&Token::Shared);
        let decls = self.parse_dim_decls()?;
        Ok(Stmt::Dim { decls, shared })
    }

    fn parse_redim(&mut self) -> Result<Stmt, String> {
        self.advance();
        let preserve = self.matches(&Token::Preserve);
        self.matches(&Token::Shared);
        let decls = self.parse_dim_decls()?;
        if let Some(d) = decls.iter().find(|d| d.dims.is_empty()) {
            return Err(format!("REDIM {} needs array bounds", d.name));
        }
        Ok(Stmt::Redim(decls, preserve))
    }

    fn parse_shared(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut names = Vec::new();
        loop {
            let name = match self.advance() {
                Token::Identifier(n) => n,
                t => {
                    return Err(format!(
                        "Expected a variable name after SHARED but found {}",
                        describe(&t)
                    ))
                }
            };
            if self.matches(&Token::LParen) {
                self.expect(&Token::RParen, "')' after shared array name")?;
            }
            if self.matches(&Token::As) {
                let vt = self.parse_type_name("SHARED type")?;
                self.var_types.insert(name.clone(), vt);
            }
            names.push(name);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::Shared(names))
    }

    fn parse_dim_decls(&mut self) -> Result<Vec<DimDecl>, String> {
        let mut decls = Vec::new();
        loop {
            let name = match self.advance() {
                Token::Identifier(n) => n,
                t => {
                    return Err(format!(
                        "Expected a variable name in DIM but found {}",
                        describe(&t)
                    ))
                }
            };
            let mut dims = Vec::new();
            let mut is_array = false;
            if self.matches(&Token::LParen) {
                is_array = true;
                if !matches!(self.peek(), Token::RParen) {
                    loop {
                        let first = self.parse_expr()?;
                        self.expect_numeric(&first, "Array bound")?;
                        if self.matches(&Token::To) {
                            let upper = self.parse_expr()?;
                            self.expect_numeric(&upper, "Array bound")?;
                            dims.push((Some(first), upper));
                        } else {
                            dims.push((None, first));
                        }
                        if !self.matches(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen, "')' after array bounds")?;
            }
            let vt = if self.matches(&Token::As) {
                let vt = self.parse_type_name("type in DIM")?;
                if let VarType::UserType(tn) = &vt {
                    if !self.type_defs.contains_key(tn) {
                        return Err(format!("Unknown type '{}'", tn));
                    }
                }
                vt
            } else if let Some(existing) = self.var_types.get(&name).cloned() {
                existing
            } else {
                self.infer_type_for_name(&name)
            };
            self.var_types.insert(name.clone(), vt.clone());
            decls.push(DimDecl {
                name,
                dims,
                vtype: vt,
                is_array,
            });
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(decls)
    }

    fn parse_erase(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut names = Vec::new();
        loop {
            match self.advance() {
                Token::Identifier(n) => names.push(n),
                t => {
                    return Err(format!(
                        "Expected an array name in ERASE but found {}",
                        describe(&t)
                    ))
                }
            }
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::Erase(names))
    }

    fn parse_data(&mut self) -> Result<Stmt, String> {
        self.advance();
        let items = match self.advance() {
            Token::DataItems(items) => items
                .into_iter()
                .map(|(text, quoted)| {
                    if quoted {
                        DataItem::Quoted(text)
                    } else {
                        DataItem::Unquoted(text)
                    }
                })
                .collect(),
            t => return Err(format!("Malformed DATA statement near {}", describe(&t))),
        };
        Ok(Stmt::Data(items))
    }

    fn parse_read(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut vars = Vec::new();
        loop {
            let lv = self.parse_lvalue()?;
            self.declare_lvalue_if_needed(&lv);
            vars.push(lv);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::Read(vars))
    }

    fn parse_restore(&mut self) -> Result<Stmt, String> {
        self.advance();
        let target = if self.at_stmt_end() {
            None
        } else {
            Some(self.parse_goto_target()?)
        };
        Ok(Stmt::Restore(target))
    }

    fn parse_const(&mut self) -> Result<Stmt, String> {
        self.advance();
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a name after CONST but found {}",
                    describe(&t)
                ))
            }
        };
        self.expect(&Token::Equals, "'=' in CONST")?;
        let e = self.parse_expr()?;
        let t = self.expr_type(&e);
        self.var_types.insert(name.clone(), t.clone());
        self.constants.insert(name.clone(), t);
        Ok(Stmt::Const(name, e))
    }

    fn parse_declare(&mut self) -> Result<Stmt, String> {
        self.advance();
        let is_func = match self.advance() {
            Token::Sub => false,
            Token::Function => true,
            t => {
                return Err(format!(
                    "Expected SUB or FUNCTION after DECLARE but found {}",
                    describe(&t)
                ))
            }
        };
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a name after DECLARE but found {}",
                    describe(&t)
                ))
            }
        };
        let params = self.parse_params()?;
        let ret_type = if is_func {
            if self.matches(&Token::As) {
                self.parse_type_name("return type")?
            } else {
                self.infer_type_for_name(&name)
            }
        } else {
            VarType::Auto
        };
        self.sub_sigs.insert(name, (params, is_func, ret_type));
        self.consume_to_line_end();
        Ok(Stmt::Noop)
    }

    fn parse_option(&mut self) -> Result<Stmt, String> {
        self.advance();
        if self.matches(&Token::Base) {
            if self.at_stmt_end() {
                return Err("Expected OPTION BASE value".to_string());
            }
            Ok(Stmt::OptionBase(self.parse_expr()?))
        } else {
            self.consume_to_line_end();
            Ok(Stmt::Noop)
        }
    }

    fn parse_def_type(&mut self) -> Result<Stmt, String> {
        let vtype = match self.advance() {
            Token::DefInt => VarType::Integer,
            Token::DefLng => VarType::Long,
            Token::DefSng => VarType::Single,
            Token::DefDbl => VarType::Double,
            _ => VarType::Str,
        };
        let mut ranges = Vec::new();
        loop {
            let letter = |t: Token| match t {
                Token::Identifier(s) if s.len() == 1 => Ok(s.chars().next().unwrap()),
                t => Err(format!(
                    "Expected a letter range but found {}",
                    describe(&t)
                )),
            };
            let start = letter(self.advance())?;
            let end = if self.matches(&Token::Minus) {
                letter(self.advance())?
            } else {
                start
            };
            ranges.push((start, end));
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        for (start, end) in &ranges {
            self.default_types.push((*start, *end, vtype.clone()));
        }
        Ok(Stmt::DefType(vtype, ranges))
    }

    fn parse_type_def(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a name after TYPE but found {}",
                    describe(&t)
                ))
            }
        };
        let mut fields = Vec::new();
        loop {
            self.skip_separators();
            if self.at_block_boundary(&[Token::EndType, Token::Eof]) {
                self.consume_line_markers();
                break;
            }
            self.consume_line_markers();
            let fname = match self.advance() {
                Token::Identifier(n) => n,
                t => {
                    return Err(format!(
                        "Expected a field name in TYPE but found {}",
                        describe(&t)
                    ))
                }
            };
            self.expect(&Token::As, "AS in TYPE field")?;
            let vt = self.parse_type_name("field type")?;
            fields.push((fname, vt));
        }
        if !self.matches(&Token::EndType) {
            return Err(format!("Line {}: TYPE without END TYPE", start_line));
        }
        self.type_defs.insert(name.clone(), fields.clone());
        Ok(Stmt::TypeDef(name, fields))
    }

    fn parse_sub(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a name after SUB but found {}",
                    describe(&t)
                ))
            }
        };
        let params = self.parse_params()?;
        let is_static = self.matches(&Token::Static);
        let saved_types = self.var_types.clone();
        for p in &params {
            self.var_types.insert(p.name.clone(), p.vtype.clone());
        }
        self.sub_sigs
            .insert(name.clone(), (params.clone(), false, VarType::Auto));
        let body = self.parse_block(&[Token::EndSub])?;
        if !self.matches(&Token::EndSub) {
            return Err(format!("Line {}: SUB without END SUB", start_line));
        }
        self.var_types = saved_types;
        Ok(Stmt::SubDef {
            name,
            params,
            body,
            is_static,
        })
    }

    fn parse_function(&mut self) -> Result<Stmt, String> {
        let start_line = self.line_here();
        self.advance();
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a name after FUNCTION but found {}",
                    describe(&t)
                ))
            }
        };
        let params = self.parse_params()?;
        let ret_type = if self.matches(&Token::As) {
            self.parse_type_name("return type")?
        } else {
            self.infer_type_for_name(&name)
        };
        let is_static = self.matches(&Token::Static);
        let saved_types = self.var_types.clone();
        for p in &params {
            self.var_types.insert(p.name.clone(), p.vtype.clone());
        }
        self.var_types.insert(name.clone(), ret_type.clone());
        self.sub_sigs
            .insert(name.clone(), (params.clone(), true, ret_type.clone()));
        let body = self.parse_block(&[Token::EndFunction])?;
        if !self.matches(&Token::EndFunction) {
            return Err(format!(
                "Line {}: FUNCTION without END FUNCTION",
                start_line
            ));
        }
        self.var_types = saved_types;
        Ok(Stmt::FuncDef {
            name,
            params,
            body,
            ret_type,
            is_static,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        if !self.matches(&Token::LParen) {
            return Ok(params);
        }
        if !matches!(self.peek(), Token::RParen) {
            loop {
                let by_ref = if self.matches(&Token::ByVal) {
                    false
                } else {
                    self.matches(&Token::ByRef);
                    true
                };
                let pname = match self.advance() {
                    Token::Identifier(n) => n,
                    t => {
                        return Err(format!(
                            "Expected a parameter name but found {}",
                            describe(&t)
                        ))
                    }
                };
                let mut is_array = false;
                if self.matches(&Token::LParen) {
                    self.expect(&Token::RParen, "')' for array parameter")?;
                    is_array = true;
                }
                let vtype = if self.matches(&Token::As) {
                    self.parse_type_name("parameter type")?
                } else {
                    self.infer_type_for_name(&pname)
                };
                params.push(Param {
                    name: pname,
                    vtype,
                    by_ref,
                    is_array,
                });
                if !self.matches(&Token::Comma) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen, "')' after parameters")?;
        Ok(params)
    }

    fn parse_type_name(&mut self, context: &str) -> Result<VarType, String> {
        match self.advance() {
            Token::StringKw => {
                if self.matches(&Token::Star) {
                    match self.advance() {
                        Token::Number(n) if (1.0..=32767.0).contains(&n) => {
                            Ok(VarType::FixedStr(n as usize))
                        }
                        t => Err(format!(
                            "Expected a string length (1-32767) after STRING * but found {}",
                            describe(&t)
                        )),
                    }
                } else {
                    Ok(VarType::Str)
                }
            }
            t => token_type(&t)
                .ok_or_else(|| format!("Expected {} but found {}", context, describe(&t))),
        }
    }

    fn parse_call(&mut self) -> Result<Stmt, String> {
        self.advance();
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => {
                return Err(format!(
                    "Expected a SUB name after CALL but found {}",
                    describe(&t)
                ))
            }
        };
        let mut args = Vec::new();
        if self.matches(&Token::LParen) {
            if !matches!(self.peek(), Token::RParen) {
                args = self.parse_expr_list()?;
            }
            self.expect(&Token::RParen, "')'")?;
        }
        Ok(Stmt::CallSub(name, args))
    }

    fn parse_on(&mut self) -> Result<Stmt, String> {
        self.advance();
        if self.matches(&Token::Error) {
            self.expect(&Token::Goto, "GOTO after ON ERROR")?;
            return match self.advance() {
                Token::Number(0.0) => Ok(Stmt::OnError(None)),
                Token::Number(n) => Ok(Stmt::OnError(Some(format!("{}", n as u32)))),
                Token::Identifier(s) => Ok(Stmt::OnError(Some(s))),
                t => Err(format!(
                    "Expected a label after ON ERROR GOTO but found {}",
                    describe(&t)
                )),
            };
        }
        let selector = self.parse_expr()?;
        let is_gosub = if self.matches(&Token::Goto) {
            false
        } else if self.matches(&Token::Gosub) {
            true
        } else {
            return Err(format!(
                "Expected GOTO or GOSUB in ON statement but found {}",
                describe(self.peek())
            ));
        };
        let mut targets = Vec::new();
        loop {
            targets.push(self.parse_goto_target()?);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::OnJump {
            selector,
            targets,
            is_gosub,
        })
    }

    fn parse_resume(&mut self) -> Result<Stmt, String> {
        self.advance();
        let kind = match self.peek().clone() {
            Token::Next => {
                self.advance();
                ResumeKind::Next
            }
            Token::Number(0.0) => {
                self.advance();
                ResumeKind::Retry
            }
            Token::Number(_) | Token::Identifier(_) => ResumeKind::Label(self.parse_goto_target()?),
            _ => ResumeKind::Retry,
        };
        Ok(Stmt::Resume(kind))
    }

    fn parse_open(&mut self) -> Result<Stmt, String> {
        self.advance();
        let filename = self.parse_expr()?;
        self.expect(&Token::For, "FOR after OPEN file name")?;
        let mode = match self.advance() {
            Token::Input => FileMode::Input,
            Token::Output => FileMode::Output,
            Token::Append => FileMode::Append,
            Token::Random => FileMode::Random,
            t => {
                return Err(format!(
                    "Expected INPUT, OUTPUT, APPEND or RANDOM but found {}",
                    describe(&t)
                ))
            }
        };
        self.expect(&Token::As, "AS in OPEN")?;
        self.matches(&Token::Hash);
        let handle = self.parse_expr()?;
        let record_len = if self.matches(&Token::LenKw) {
            self.expect(&Token::Equals, "'=' after LEN")?;
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Open {
            filename,
            mode,
            handle,
            record_len,
        })
    }

    fn parse_close(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut handles = Vec::new();
        while !self.at_stmt_end() {
            self.matches(&Token::Hash);
            handles.push(self.parse_expr()?);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(Stmt::Close(handles))
    }

    fn parse_get_put(&mut self, is_get: bool) -> Result<Stmt, String> {
        let kw = if is_get { "GET" } else { "PUT" };
        self.advance();
        self.matches(&Token::Hash);
        let handle = self.parse_expr()?;
        self.expect(&Token::Comma, &format!("',' after file number in {}", kw))?;
        let record = if matches!(self.peek(), Token::Comma) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&Token::Comma, &format!("',' before variable in {}", kw))?;
        let var = self.parse_lvalue()?;
        self.declare_lvalue_if_needed(&var);
        Ok(if is_get {
            Stmt::Get {
                handle,
                record,
                var,
            }
        } else {
            Stmt::Put {
                handle,
                record,
                var,
            }
        })
    }

    fn parse_lvalue(&mut self) -> Result<LValue, String> {
        let name = match self.advance() {
            Token::Identifier(n) => n,
            t => return Err(format!("Expected a variable but found {}", describe(&t))),
        };
        let mut lv = if self.matches(&Token::LParen) {
            let mut args = Vec::new();
            if !matches!(self.peek(), Token::RParen) {
                args = self.parse_expr_list()?;
            }
            self.expect(&Token::RParen, "')'")?;
            LValue::Index(name, args)
        } else {
            LValue::Var(name)
        };
        while self.matches(&Token::Dot) {
            let f = match self.advance() {
                Token::Identifier(s) => s,
                t => {
                    return Err(format!(
                        "Expected a field name after '.' but found {}",
                        describe(&t)
                    ))
                }
            };
            lv = LValue::Field(Box::new(lv), f);
        }
        Ok(lv)
    }

    fn parse_point(&mut self) -> Result<(Expr, Expr), String> {
        self.expect(&Token::LParen, "'('")?;
        let x = self.parse_expr()?;
        self.expect(&Token::Comma, "','")?;
        let y = self.parse_expr()?;
        self.expect(&Token::RParen, "')'")?;
        Ok((x, y))
    }

    fn parse_color(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut fg = None;
        let mut bg = None;
        if !self.at_stmt_end() && !matches!(self.peek(), Token::Comma) {
            fg = Some(self.parse_expr()?);
        }
        if self.matches(&Token::Comma) && !self.at_stmt_end() {
            bg = Some(self.parse_expr()?);
        }
        self.consume_to_line_end();
        Ok(Stmt::Color(fg, bg))
    }

    fn parse_pset(&mut self) -> Result<Stmt, String> {
        self.advance();
        let (x, y) = self.parse_point()?;
        let color = if self.matches(&Token::Comma) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Pset(x, y, color))
    }

    fn parse_line_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let p1 = if matches!(self.peek(), Token::LParen) {
            Some(self.parse_point()?)
        } else {
            None
        };
        self.expect(&Token::Minus, "'-' in LINE")?;
        let (x2, y2) = self.parse_point()?;
        let mut color = None;
        let mut is_box = false;
        let mut is_filled = false;
        if self.matches(&Token::Comma) {
            if !matches!(self.peek(), Token::Comma) && !self.at_stmt_end() {
                color = Some(self.parse_expr()?);
            }
            if self.matches(&Token::Comma) {
                if let Token::Identifier(s) = self.peek().clone() {
                    match s.as_str() {
                        "B" => {
                            self.advance();
                            is_box = true;
                        }
                        "BF" => {
                            self.advance();
                            is_box = true;
                            is_filled = true;
                        }
                        _ => {}
                    }
                }
                // Line style argument is accepted and ignored.
                if self.matches(&Token::Comma) {
                    let _ = self.parse_expr()?;
                }
            }
        }
        Ok(Stmt::Line(p1, x2, y2, color, is_box, is_filled))
    }

    fn parse_circle(&mut self) -> Result<Stmt, String> {
        self.advance();
        let (x, y) = self.parse_point()?;
        self.expect(&Token::Comma, "',' before CIRCLE radius")?;
        let r = self.parse_expr()?;
        let color = if self.matches(&Token::Comma) && !matches!(self.peek(), Token::Comma) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        // Arc start/end and aspect arguments are not supported yet.
        self.consume_to_line_end();
        Ok(Stmt::Circle(x, y, r, color))
    }

    fn parse_paint(&mut self) -> Result<Stmt, String> {
        self.advance();
        let (x, y) = self.parse_point()?;
        let mut color = None;
        let mut border = None;
        if self.matches(&Token::Comma) {
            if !matches!(self.peek(), Token::Comma) && !self.at_stmt_end() {
                color = Some(self.parse_expr()?);
            }
            if self.matches(&Token::Comma) {
                border = Some(self.parse_expr()?);
            }
        }
        Ok(Stmt::Paint(x, y, color, border))
    }

    fn parse_locate(&mut self) -> Result<Stmt, String> {
        self.advance();
        let row = if self.at_stmt_end() || matches!(self.peek(), Token::Comma) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        let col = if self.matches(&Token::Comma)
            && !self.at_stmt_end()
            && !matches!(self.peek(), Token::Comma)
        {
            Some(self.parse_expr()?)
        } else {
            None
        };
        // Cursor visibility/shape arguments are accepted and ignored.
        self.consume_to_line_end();
        Ok(Stmt::Locate(row, col))
    }

    fn parse_swap(&mut self) -> Result<Stmt, String> {
        self.advance();
        let left = self.parse_lvalue()?;
        self.expect(&Token::Comma, "',' in SWAP")?;
        let right = self.parse_lvalue()?;
        self.declare_lvalue_if_needed(&left);
        self.declare_lvalue_if_needed(&right);
        if self.lvalue_type(&left).is_string() != self.lvalue_type(&right).is_string() {
            return Err("Type mismatch: SWAP needs two variables of the same type".to_string());
        }
        Ok(Stmt::Swap(left, right))
    }

    fn parse_randomize(&mut self) -> Result<Stmt, String> {
        self.advance();
        let seed = if self.at_stmt_end() {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Randomize(seed))
    }

    // ---- Expressions -------------------------------------------------------

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            self.depth -= 1;
            return Err("Expression is too deeply nested".to_string());
        }
        let result = self.parse_xor();
        self.depth -= 1;
        let e = result?;
        if self.depth == 0 {
            self.check_expr(&e)?;
        }
        Ok(e)
    }
    fn parse_xor(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_or()?;
        while self.matches(&Token::Xor) {
            let r = self.parse_or()?;
            left = Expr::BinaryOp(Box::new(left), BinOp::Xor, Box::new(r));
        }
        Ok(left)
    }
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.matches(&Token::Or) {
            let r = self.parse_and()?;
            left = Expr::BinaryOp(Box::new(left), BinOp::Or, Box::new(r));
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_not()?;
        while self.matches(&Token::And) {
            let r = self.parse_not()?;
            left = Expr::BinaryOp(Box::new(left), BinOp::And, Box::new(r));
        }
        Ok(left)
    }
    fn parse_not(&mut self) -> Result<Expr, String> {
        if self.matches(&Token::Not) {
            let inner = self.parse_not()?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.parse_comparison()
    }
    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_addsub()?;
        loop {
            let op = match self.peek() {
                Token::Equals => BinOp::Eq,
                Token::NotEqual => BinOp::NotEq,
                Token::Less => BinOp::Lt,
                Token::LessEqual => BinOp::Le,
                Token::Greater => BinOp::Gt,
                Token::GreaterEqual => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let r = self.parse_addsub()?;
            left = Expr::BinaryOp(Box::new(left), op, Box::new(r));
        }
        Ok(left)
    }
    fn parse_addsub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mod()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let r = self.parse_mod()?;
            left = Expr::BinaryOp(Box::new(left), op, Box::new(r));
        }
        Ok(left)
    }
    fn parse_mod(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_intdiv()?;
        while self.matches(&Token::Mod) {
            let r = self.parse_intdiv()?;
            left = Expr::BinaryOp(Box::new(left), BinOp::Mod, Box::new(r));
        }
        Ok(left)
    }
    fn parse_intdiv(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_muldiv()?;
        while self.matches(&Token::Backslash) {
            let r = self.parse_muldiv()?;
            left = Expr::BinaryOp(Box::new(left), BinOp::IntDiv, Box::new(r));
        }
        Ok(left)
    }
    fn parse_muldiv(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let r = self.parse_unary()?;
            left = Expr::BinaryOp(Box::new(left), op, Box::new(r));
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.matches(&Token::Minus) {
            let inner = self.parse_unary()?;
            return Ok(Expr::UnaryMinus(Box::new(inner)));
        }
        if self.matches(&Token::Plus) {
            return self.parse_unary();
        }
        self.parse_pow()
    }
    /// `^` binds tighter than unary minus and is left-associative, as in
    /// QBasic: `-2 ^ 2` is -4 and `2 ^ 3 ^ 2` is 64.
    fn parse_pow(&mut self) -> Result<Expr, String> {
        let mut base = self.parse_primary()?;
        while self.matches(&Token::Caret) {
            let exp = if self.matches(&Token::Minus) {
                Expr::UnaryMinus(Box::new(self.parse_primary()?))
            } else {
                self.matches(&Token::Plus);
                self.parse_primary()?
            };
            base = Expr::BinaryOp(Box::new(base), BinOp::Pow, Box::new(exp));
        }
        Ok(base)
    }
    fn parse_primary(&mut self) -> Result<Expr, String> {
        let mut e = match self.advance() {
            Token::Number(n) => Expr::Number(n),
            Token::StringLit(s) => Expr::StringLit(s),
            Token::LenKw => {
                self.expect(&Token::LParen, "'(' after LEN")?;
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen, "')'")?;
                Expr::ArrayOrCall("LEN".to_string(), vec![inner])
            }
            Token::Identifier(name) => {
                if self.matches(&Token::LParen) {
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        args = self.parse_expr_list()?;
                    }
                    self.expect(&Token::RParen, "')'")?;
                    Expr::ArrayOrCall(name, args)
                } else {
                    Expr::Variable(name)
                }
            }
            Token::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen, "')'")?;
                e
            }
            Token::Minus => Expr::UnaryMinus(Box::new(self.parse_unary()?)),
            t => {
                return Err(format!(
                    "Expected a value or expression but found {}",
                    describe(&t)
                ))
            }
        };
        while self.matches(&Token::Dot) {
            let f = match self.advance() {
                Token::Identifier(s) => s,
                t => {
                    return Err(format!(
                        "Expected a field name after '.' but found {}",
                        describe(&t)
                    ))
                }
            };
            e = Expr::FieldAccess(Box::new(e), f);
        }
        Ok(e)
    }

    // ---- Static typing helpers --------------------------------------------

    fn declare_var_if_needed(&mut self, name: &str) {
        if !self.var_types.contains_key(name) {
            self.var_types
                .insert(name.to_string(), self.infer_type_for_name(name));
        }
    }
    fn declare_lvalue_if_needed(&mut self, lv: &LValue) {
        match lv {
            LValue::Var(name) | LValue::Index(name, _) => self.declare_var_if_needed(name),
            LValue::Field(parent, _) => self.declare_lvalue_if_needed(parent),
        }
    }
    fn lookup_var_type(&self, name: &str) -> VarType {
        self.var_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.infer_type_for_name(name))
    }
    fn infer_type_for_name(&self, name: &str) -> VarType {
        infer_type_with_defaults(name, &self.default_types)
    }
    fn field_type(&self, parent: VarType, fname: &str) -> VarType {
        if let VarType::UserType(tn) = parent {
            if let Some(fields) = self.type_defs.get(&tn) {
                if let Some((_, ft)) = fields.iter().find(|(n, _)| n == fname) {
                    return ft.clone();
                }
            }
            return VarType::Auto;
        }
        VarType::Auto
    }
    fn lvalue_type(&self, lv: &LValue) -> VarType {
        match lv {
            LValue::Var(n) | LValue::Index(n, _) => self.lookup_var_type(n),
            LValue::Field(parent, fname) => self.field_type(self.lvalue_type(parent), fname),
        }
    }
    fn expr_type(&self, e: &Expr) -> VarType {
        match e {
            Expr::Number(_) => VarType::Double,
            Expr::StringLit(_) => VarType::Str,
            Expr::Variable(n) => {
                if ZERO_ARG_BUILTINS.contains(&n.as_str()) {
                    return builtin_return_type(n).unwrap_or(VarType::Auto);
                }
                if let Some(t) = self.constants.get(n) {
                    return t.clone();
                }
                if let Some(t) = self.var_types.get(n) {
                    return t.clone();
                }
                if let Some((p, true, rt)) = self.sub_sigs.get(n) {
                    if p.is_empty() {
                        return rt.clone();
                    }
                }
                self.infer_type_for_name(n)
            }
            Expr::FieldAccess(parent, fname) => self.field_type(self.expr_type(parent), fname),
            Expr::ArrayOrCall(name, _) => {
                if let Some(rt) = builtin_return_type(name) {
                    return rt;
                }
                if let Some((_, _, rt)) = self.sub_sigs.get(name) {
                    return rt.clone();
                }
                self.lookup_var_type(name)
            }
            Expr::UnaryMinus(_) | Expr::Not(_) => VarType::Double,
            Expr::BinaryOp(l, op, r) => match op {
                BinOp::Add => {
                    if self.expr_type(l).is_string() || self.expr_type(r).is_string() {
                        VarType::Str
                    } else {
                        VarType::Double
                    }
                }
                _ => VarType::Double,
            },
        }
    }
    /// Rejects expressions that mix strings and numbers, which would otherwise
    /// only fail at runtime.
    fn check_expr(&self, e: &Expr) -> Result<(), String> {
        match e {
            Expr::BinaryOp(l, op, r) => {
                self.check_expr(l)?;
                self.check_expr(r)?;
                let lt = self.expr_type(l);
                let rt = self.expr_type(r);
                let known = |t: &VarType| t.is_string() || t.is_numeric();
                if !known(&lt) || !known(&rt) {
                    return Ok(());
                }
                if lt.is_string() != rt.is_string() {
                    return Err(format!(
                        "Type mismatch: cannot use {} between a string and a number",
                        op.symbol()
                    ));
                }
                if lt.is_string() && !(*op == BinOp::Add || op.is_comparison()) {
                    return Err(format!(
                        "Type mismatch: {} cannot be used with strings",
                        op.symbol()
                    ));
                }
                Ok(())
            }
            Expr::UnaryMinus(inner) | Expr::Not(inner) => {
                self.check_expr(inner)?;
                if self.expr_type(inner).is_string() {
                    return Err("Type mismatch: cannot negate a string".to_string());
                }
                Ok(())
            }
            Expr::FieldAccess(inner, _) => self.check_expr(inner),
            Expr::ArrayOrCall(_, args) => args.iter().try_for_each(|a| self.check_expr(a)),
            _ => Ok(()),
        }
    }
    fn check_assignment_types(&self, lv: &LValue, e: &Expr) -> Result<(), String> {
        let lt = self.lvalue_type(lv);
        let et = self.expr_type(e);
        if lt.is_string() && et.is_numeric() {
            return Err(format!(
                "Type mismatch: cannot assign a number to string variable {}",
                lv.root()
            ));
        }
        if lt.is_numeric() && et.is_string() {
            return Err(format!(
                "Type mismatch: cannot assign a string to numeric variable {}",
                lv.root()
            ));
        }
        Ok(())
    }
    fn expect_numeric(&self, e: &Expr, ctx: &str) -> Result<(), String> {
        if self.expr_type(e).is_string() {
            return Err(format!("{} requires a numeric expression", ctx));
        }
        Ok(())
    }
}

fn token_type(t: &Token) -> Option<VarType> {
    Some(match t {
        Token::Integer => VarType::Integer,
        Token::Long => VarType::Long,
        Token::Single => VarType::Single,
        Token::Double => VarType::Double,
        Token::StringKw => VarType::Str,
        Token::Identifier(s) => VarType::UserType(s.clone()),
        _ => return None,
    })
}

pub fn infer_type_from_suffix(name: &str) -> VarType {
    match name.chars().last() {
        Some('$') => VarType::Str,
        Some('%') => VarType::Integer,
        Some('&') => VarType::Long,
        Some('#') => VarType::Double,
        _ => VarType::Single,
    }
}

/// Type of an undeclared variable: its suffix, else the matching DEFtype
/// range, else SINGLE.
pub fn infer_type_with_defaults(name: &str, defaults: &[(char, char, VarType)]) -> VarType {
    let suffixed = infer_type_from_suffix(name);
    if !matches!(suffixed, VarType::Single) || name.ends_with('!') {
        return suffixed;
    }
    let Some(first) = name.chars().next() else {
        return suffixed;
    };
    let first = first.to_ascii_uppercase();
    defaults
        .iter()
        .rev()
        .find(|(start, end, _)| first >= *start && first <= *end)
        .map(|(_, _, vt)| vt.clone())
        .unwrap_or(suffixed)
}

pub fn builtin_return_type(name: &str) -> Option<VarType> {
    let t = match name {
        "LEN" | "INT" | "FIX" | "ABS" | "SQR" | "RND" | "VAL" | "ASC" | "INSTR" | "SIN" | "COS"
        | "TAN" | "ATN" | "LOG" | "EXP" | "SGN" | "TIMER" | "CINT" | "CLNG" | "CSNG" | "CDBL"
        | "UBOUND" | "LBOUND" | "EOF" | "LOF" | "FREEFILE" | "ERR" | "ERL" | "CSRLIN" | "POS" => {
            VarType::Double
        }
        "MID$" | "LEFT$" | "RIGHT$" | "STR$" | "CHR$" | "UCASE$" | "LCASE$" | "LTRIM$"
        | "RTRIM$" | "TRIM$" | "SPACE$" | "STRING$" | "HEX$" | "OCT$" | "INKEY$" | "DATE$"
        | "TIME$" | "INPUT$" | "CSTR$" | "TAB" | "SPC" => VarType::Str,
        _ => return None,
    };
    Some(t)
}

pub fn is_builtin(name: &str) -> bool {
    builtin_return_type(name).is_some()
}

// ---- Semantic validation ---------------------------------------------------

struct LabelSite {
    routine: usize,
    block: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum JumpKind {
    Goto,
    Gosub,
    Restore,
    Handler,
}

struct Jump {
    target: String,
    routine: usize,
    block: usize,
    line: usize,
    kind: JumpKind,
}

struct Validator<'a> {
    sub_sigs: &'a HashMap<String, SubSig>,
    labels: HashMap<String, LabelSite>,
    jumps: Vec<Jump>,
    /// Parent of each block id; routine root blocks have no parent.
    parents: Vec<Option<usize>>,
    /// Root block of each routine (0 = main program).
    roots: Vec<usize>,
    line: usize,
}

fn label_name(t: &str) -> String {
    if t.chars().all(|c| c.is_ascii_digit()) {
        format!("line number {}", t)
    } else {
        format!("label {}", t)
    }
}

fn capitalize(s: String) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// Checks jump targets, SUB calls and EXIT placement before the program runs,
/// so mistakes are reported with a line number instead of at runtime.
fn validate(stmts: &[Stmt], sub_sigs: &HashMap<String, SubSig>) -> Result<(), String> {
    let mut v = Validator {
        sub_sigs,
        labels: HashMap::new(),
        jumps: Vec::new(),
        parents: vec![None],
        roots: vec![0],
        line: 1,
    };
    v.walk(stmts, 0, 0, &mut Vec::new())
        .map_err(|e| format!("Line {}: {}", v.line, e))?;
    for j in &v.jumps {
        v.check_jump(j)
            .map_err(|e| format!("Line {}: {}", j.line, e))?;
    }
    Ok(())
}

fn child_blocks(s: &Stmt) -> Vec<&[Stmt]> {
    match s {
        Stmt::If {
            then_branch,
            elseifs,
            else_branch,
            ..
        } => {
            let mut v: Vec<&[Stmt]> = vec![then_branch];
            v.extend(elseifs.iter().map(|(_, b)| b.as_slice()));
            v.push(else_branch);
            v
        }
        Stmt::SelectCase {
            cases, else_branch, ..
        } => {
            let mut v: Vec<&[Stmt]> = cases.iter().map(|(_, b)| b.as_slice()).collect();
            v.push(else_branch);
            v
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::DoLoop { body, .. } => {
            vec![body]
        }
        _ => Vec::new(),
    }
}

impl Validator<'_> {
    fn new_block(&mut self, parent: Option<usize>) -> usize {
        self.parents.push(parent);
        self.parents.len() - 1
    }

    fn add_label(&mut self, name: String, routine: usize, block: usize) -> Result<(), String> {
        if self.labels.contains_key(&name) {
            return Err(format!("Duplicate label: {}", name));
        }
        self.labels.insert(name, LabelSite { routine, block });
        Ok(())
    }

    fn jump(&mut self, target: &str, routine: usize, block: usize, kind: JumpKind) {
        self.jumps.push(Jump {
            target: target.to_string(),
            routine,
            block,
            line: self.line,
            kind,
        });
    }

    fn walk(
        &mut self,
        stmts: &[Stmt],
        routine: usize,
        block: usize,
        context: &mut Vec<&'static str>,
    ) -> Result<(), String> {
        for s in stmts {
            match s {
                Stmt::SrcLine(l) => self.line = *l,
                Stmt::Label(n) => self.add_label(n.clone(), routine, block)?,
                Stmt::LineNumber(n) => self.add_label(n.to_string(), routine, block)?,
                Stmt::Goto(t) => self.jump(t, routine, block, JumpKind::Goto),
                Stmt::Gosub(t) => self.jump(t, routine, block, JumpKind::Gosub),
                Stmt::OnJump {
                    targets, is_gosub, ..
                } => {
                    let kind = if *is_gosub {
                        JumpKind::Gosub
                    } else {
                        JumpKind::Goto
                    };
                    for t in targets {
                        self.jump(t, routine, block, kind);
                    }
                }
                Stmt::Restore(Some(t)) => self.jump(t, routine, block, JumpKind::Restore),
                Stmt::OnError(Some(t)) | Stmt::Resume(ResumeKind::Label(t)) => {
                    self.jump(t, routine, block, JumpKind::Handler)
                }
                Stmt::CallSub(name, args) => match self.sub_sigs.get(name) {
                    None => return Err(format!("Unknown statement or SUB '{}'", name)),
                    Some((params, _, _)) if params.len() != args.len() => {
                        return Err(format!(
                            "{} expects {} argument(s) but got {}",
                            name,
                            params.len(),
                            args.len()
                        ));
                    }
                    _ => {}
                },
                Stmt::Exit(kind) => {
                    let ok = match kind.as_str() {
                        "FOR" | "DO" | "WHILE" => context.contains(&kind.as_str()),
                        k => context.first() == Some(&k),
                    };
                    if !ok {
                        return Err(format!("EXIT {} is not inside a {} block", kind, kind));
                    }
                }
                Stmt::SubDef { body, .. } | Stmt::FuncDef { body, .. } => {
                    if routine != 0 {
                        return Err("SUB and FUNCTION cannot be nested".to_string());
                    }
                    let root = self.new_block(None);
                    self.roots.push(root);
                    let r = self.roots.len() - 1;
                    let kind = if matches!(s, Stmt::SubDef { .. }) {
                        "SUB"
                    } else {
                        "FUNCTION"
                    };
                    self.walk(body, r, root, &mut vec![kind])?;
                }
                other => {
                    let kind = match other {
                        Stmt::For { .. } => Some("FOR"),
                        Stmt::DoLoop { .. } => Some("DO"),
                        Stmt::While { .. } => Some("WHILE"),
                        _ => None,
                    };
                    if let Some(k) = kind {
                        context.push(k);
                    }
                    for b in child_blocks(other) {
                        let id = self.new_block(Some(block));
                        self.walk(b, routine, id, context)?;
                    }
                    if kind.is_some() {
                        context.pop();
                    }
                }
            }
        }
        Ok(())
    }

    fn is_ancestor(&self, ancestor: usize, mut block: usize) -> bool {
        loop {
            if block == ancestor {
                return true;
            }
            match self.parents[block] {
                Some(p) => block = p,
                None => return false,
            }
        }
    }

    fn check_jump(&self, j: &Jump) -> Result<(), String> {
        let name = label_name(&j.target);
        let site = self
            .labels
            .get(&j.target)
            .ok_or_else(|| format!("{} is not defined", capitalize(name.clone())))?;
        match j.kind {
            JumpKind::Restore => Ok(()),
            JumpKind::Handler => {
                if site.routine != 0 || site.block != self.roots[0] {
                    return Err(format!(
                        "Error handler {} must be at the top level of the main program",
                        name
                    ));
                }
                Ok(())
            }
            JumpKind::Goto | JumpKind::Gosub => {
                if site.routine != j.routine {
                    return Err(format!(
                        "{} is in a different SUB or FUNCTION; jumps cannot cross procedures",
                        capitalize(name)
                    ));
                }
                if j.kind == JumpKind::Gosub {
                    if site.block != self.roots[j.routine] {
                        return Err(format!(
                            "GOSUB target {} must not be inside a block (IF, FOR, DO, ...)",
                            name
                        ));
                    }
                } else if !self.is_ancestor(site.block, j.block) {
                    return Err(format!(
                        "Cannot jump into a block: {} is inside an IF, FOR, DO, WHILE or SELECT that the jump is not in",
                        name
                    ));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_src(src: &str) -> Result<Program, String> {
        parse(tokenize(src)?)
    }

    #[test]
    fn one_line_blocks_parse() {
        parse_src("FOR I = 1 TO 3: PRINT I: NEXT I\n").unwrap();
        parse_src("I = 0\nWHILE I < 3: I = I + 1: WEND\n").unwrap();
        parse_src("DO: I = I + 1: LOOP UNTIL I > 3\n").unwrap();
        parse_src(
            "X = 1\nSELECT CASE X\nCASE 1: PRINT \"one\"\nCASE ELSE: PRINT \"x\"\nEND SELECT\n",
        )
        .unwrap();
    }

    #[test]
    fn next_with_multiple_variables() {
        parse_src("FOR I = 1 TO 2\nFOR J = 1 TO 2\nPRINT I; J\nNEXT J, I\n").unwrap();
        let err = parse_src("FOR I = 1 TO 2\nNEXT J\n").unwrap_err();
        assert!(err.contains("NEXT J does not match FOR I"), "{}", err);
    }

    #[test]
    fn missing_block_end_reports_start_line() {
        let err = parse_src("PRINT 1\nIF X THEN\nPRINT 2\n").unwrap_err();
        assert_eq!(err, "Line 2: Block IF without END IF");
        let err = parse_src("PRINT 1\n\nFOR I = 1 TO 2\n").unwrap_err();
        assert_eq!(err, "Line 3: FOR without NEXT");
    }

    #[test]
    fn errors_carry_line_numbers() {
        let err = parse_src("PRINT 1\nX = (1 +\n").unwrap_err();
        assert!(err.starts_with("Line 2:"), "{}", err);
        let err = parse_src("PRINT 1\nPRINT 2\nNEXT\n").unwrap_err();
        assert_eq!(err, "Line 3: NEXT without FOR");
    }

    #[test]
    fn unknown_statements_and_labels_are_rejected() {
        let err = parse_src("PRNT \"hello\"\n").unwrap_err();
        assert_eq!(err, "Line 1: Unknown statement or SUB 'PRNT'");
        let err = parse_src("PRINT 1\nGOTO nowhere\n").unwrap_err();
        assert_eq!(err, "Line 2: Label NOWHERE is not defined");
        let err = parse_src("GOTO 30\nIF 1 THEN\n30 PRINT \"in\"\nEND IF\n").unwrap_err();
        assert!(err.contains("Cannot jump into a block"), "{}", err);
    }

    #[test]
    fn jumps_out_of_and_within_blocks_are_allowed() {
        parse_src("FOR I = 1 TO 3\nIF I = 2 THEN GOTO done\nNEXT I\ndone:\nPRINT I\n").unwrap();
        parse_src("FOR I = 1 TO 3\nGOTO 20\n10 PRINT \"skip\"\n20 NEXT I\n").unwrap();
    }

    #[test]
    fn exit_must_be_inside_matching_block() {
        assert!(parse_src("EXIT FOR\n").unwrap_err().contains("EXIT FOR"));
        parse_src("FOR I = 1 TO 2\nIF I THEN EXIT FOR\nNEXT\n").unwrap();
        parse_src("SUB S\nEXIT SUB\nEND SUB\n").unwrap();
    }

    #[test]
    fn mixed_string_and_number_expressions_are_rejected() {
        let err = parse_src("PRINT \"n=\" + 5\n").unwrap_err();
        assert!(err.contains("Type mismatch"), "{}", err);
        let err = parse_src("IF \"a\" = 1 THEN PRINT 1\n").unwrap_err();
        assert!(err.contains("Type mismatch"), "{}", err);
        parse_src("DEFSTR A-Z\nPRINT A + \"x\"\n").unwrap();
    }

    #[test]
    fn power_is_left_associative_and_binds_tighter_than_negation() {
        let p = parse_src("X = -2 ^ 2\n").unwrap();
        assert!(matches!(
            p.stmts.last(),
            Some(Stmt::Let(_, Expr::UnaryMinus(_)))
        ));
    }

    #[test]
    fn deeply_nested_expressions_error_instead_of_overflowing() {
        let src = format!("X = {}1{}\n", "(".repeat(5000), ")".repeat(5000));
        let err = parse_src(&src).unwrap_err();
        assert!(err.contains("too deeply nested"), "{}", err);
    }

    #[test]
    fn dim_with_explicit_bounds_and_record_arrays() {
        parse_src("DIM A(1 TO 10, -5 TO 5)\n").unwrap();
        parse_src(
            "TYPE P\nN AS STRING * 8\nEND TYPE\nDIM A(3) AS P\nA(1).N = \"x\"\nPRINT A(1).N\n",
        )
        .unwrap();
    }
}
