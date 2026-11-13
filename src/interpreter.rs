use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::format::{format_number, print_using, UsingArg};
use crate::graphics::{qb_color, SharedGraphics};
use crate::parser::{
    infer_type_with_defaults, is_builtin, BinOp, CaseClause, DataItem, DimDecl, Expr, FileMode,
    LValue, Param, PrintItem, Program, ResumeKind, Stmt, VarType, ZERO_ARG_BUILTINS,
};

/// Maximum nesting of SUB/FUNCTION calls and GOSUBs before
/// "Out of stack space" is raised.
pub const MAX_CALL_DEPTH: usize = 1000;
/// Stack size for the interpreter thread; deep BASIC recursion needs room.
pub const INTERPRETER_STACK_SIZE: usize = 512 * 1024 * 1024;
const MAX_ARRAY_ELEMENTS: usize = 16 * 1024 * 1024;
const MAX_STRING_LEN: usize = 16 * 1024 * 1024;
const ZONE_WIDTH: usize = 14;

/// Internal signal used to unwind an `END` executed inside a SUB/FUNCTION.
const END_SIGNAL: &str = "\u{0}END";
pub const INTERRUPTED: &str = "Program interrupted by user";

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    Str(String),
    Array(ArrayValue),
    Record(RecordValue),
}

#[derive(Debug, Clone)]
pub struct ArrayValue {
    pub dims: Vec<usize>,
    pub lower_bounds: Vec<i64>,
    pub data: Vec<Value>,
    pub element_type: VarType,
}

#[derive(Debug, Clone)]
pub struct RecordValue {
    pub type_name: String,
    pub fields: Vec<(String, Value)>,
}

impl Value {
    pub fn to_display(&self) -> Result<String, String> {
        match self {
            Value::Number(n) => Ok(format_number(*n)),
            Value::Str(s) => Ok(s.clone()),
            Value::Array(_) => Err("Type mismatch: cannot print a whole array".to_string()),
            Value::Record(_) => Err("Type mismatch: cannot print a whole record".to_string()),
        }
    }
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err("Type mismatch: expected a number".to_string()),
        }
    }
    pub fn as_string(&self) -> Result<String, String> {
        match self {
            Value::Str(s) => Ok(s.clone()),
            _ => Err("Type mismatch: expected a string".to_string()),
        }
    }
}

pub enum ConsoleMsg {
    Print {
        text: String,
        newline: bool,
    },
    /// Ask the front end for a line of keyboard input, showing this prompt.
    InputPrompt(String),
    Clear,
    /// Text-mode colour change (QBasic palette indexes 0-15).
    Color {
        fg: Option<u8>,
        bg: Option<u8>,
    },
    /// Move the text cursor (1-based); `None` keeps that coordinate.
    Locate {
        row: Option<u16>,
        col: Option<u16>,
    },
}

/// Channels and shared state connecting the interpreter to its front end.
pub struct Io {
    pub console_tx: Sender<ConsoleMsg>,
    pub input_rx: Receiver<String>,
    pub cancel: Arc<AtomicBool>,
    pub graphics: Arc<Mutex<SharedGraphics>>,
    /// Key presses for `INKEY$`, filled by the front end.
    pub keys: Arc<Mutex<VecDeque<String>>>,
    /// Whether a person can press keys (raw terminal or graphics window).
    pub interactive: bool,
}

/// Runs a program on a dedicated thread with a stack large enough for deep
/// recursion.
pub fn spawn(program: Program, io: Io) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::Builder::new()
        .name("basic-interpreter".to_string())
        .stack_size(INTERPRETER_STACK_SIZE)
        .spawn(move || run(program, io))
        .expect("failed to start interpreter thread")
}

enum Flow {
    Normal,
    Goto(String),
    Return,
    End,
    ExitFor,
    ExitDo,
    ExitWhile,
    ExitSub,
    Resume(ResumeKind),
}

enum FileHandle {
    Input(BufReader<File>),
    Output { writer: BufWriter<File>, col: usize },
    Random { file: File, record_len: usize },
}

struct Routine {
    body: Vec<Stmt>,
    labels: HashMap<String, usize>,
}

impl Routine {
    fn new(body: Vec<Stmt>) -> Rc<Self> {
        let mut labels = HashMap::new();
        for (i, s) in body.iter().enumerate() {
            match s {
                Stmt::Label(n) => {
                    labels.insert(n.clone(), i);
                }
                Stmt::LineNumber(n) => {
                    labels.insert(n.to_string(), i);
                }
                _ => {}
            }
        }
        Rc::new(Routine { body, labels })
    }
}

struct SubInfo {
    params: Vec<Param>,
    routine: Rc<Routine>,
    is_function: bool,
    ret_type: VarType,
    is_static: bool,
}

struct Frame {
    vars: HashMap<String, Value>,
    declared: HashMap<String, VarType>,
    shared: HashSet<String>,
    statics: HashSet<String>,
    routine: Rc<Routine>,
    sub_name: Option<String>,
}

impl Frame {
    fn new(routine: Rc<Routine>, sub_name: Option<String>) -> Self {
        Frame {
            vars: HashMap::new(),
            declared: HashMap::new(),
            shared: HashSet::new(),
            statics: HashSet::new(),
            routine,
            sub_name,
        }
    }
}

/// A resolved assignment target: indexes are evaluated exactly once.
enum Seg {
    Idx(Vec<i64>),
    Field(String),
}

pub struct Interpreter {
    frames: Vec<Frame>,
    subs: HashMap<String, Rc<SubInfo>>,
    types: HashMap<String, Vec<(String, VarType)>>,
    constants: HashMap<String, Value>,
    global_shared: HashSet<String>,
    static_store: HashMap<String, HashMap<String, Value>>,
    data: Vec<DataItem>,
    data_ptr: usize,
    data_labels: HashMap<String, usize>,
    files: HashMap<i64, FileHandle>,
    option_base: i64,
    default_types: Vec<(char, char, VarType)>,
    rng_state: u64,
    last_rnd: f64,
    io: Io,
    cur_line: usize,
    pending_line_number: Option<u32>,
    basic_line: Option<u32>,
    last_line_number: u32,
    depth: usize,
    on_error: Option<String>,
    in_handler: bool,
    err_code: i64,
    err_line: u32,
    cursor_col: usize,
    cursor_row: usize,
}

pub fn run(program: Program, io: Io) -> Result<(), String> {
    let mut interp = Interpreter::new(io);
    let result = interp.run_program(program.stmts);
    interp.files.clear();
    match result {
        Ok(()) => Ok(()),
        Err(e) if e == END_SIGNAL => Ok(()),
        Err(e) if e == INTERRUPTED => Err(e),
        Err(e) => Err(interp.located(&e)),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Number(n) => *n != 0.0,
        Value::Str(s) => !s.is_empty(),
        _ => false,
    }
}

/// QBasic converts to integers by rounding half to even.
fn to_int(n: f64) -> i64 {
    n.round_ties_even() as i64
}

fn bool_value(b: bool) -> Value {
    Value::Number(if b { -1.0 } else { 0.0 })
}

/// QBasic error codes and their standard messages.
const ERROR_TABLE: &[(i64, &str)] = &[
    (2, "Syntax error"),
    (3, "RETURN without GOSUB"),
    (4, "Out of DATA"),
    (5, "Illegal function call"),
    (6, "Overflow"),
    (7, "Out of memory"),
    (9, "Subscript out of range"),
    (10, "Duplicate definition"),
    (11, "Division by zero"),
    (13, "Type mismatch"),
    (14, "Out of string space"),
    (20, "RESUME without error"),
    (28, "Out of stack space"),
    (52, "Bad file name or number"),
    (53, "File not found"),
    (54, "Bad file mode"),
    (55, "File already open"),
    (59, "Bad record length"),
    (62, "Input past end of file"),
    (64, "Bad file name"),
    (75, "Path/File access error"),
    (76, "Path not found"),
];

fn error_message(code: i64) -> String {
    ERROR_TABLE
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| m.to_string())
        .unwrap_or_else(|| format!("Error {}", code))
}

fn error_code(msg: &str) -> i64 {
    if let Some(rest) = msg.strip_prefix("Error ") {
        if let Ok(n) = rest.trim().parse() {
            return n;
        }
    }
    if msg.starts_with("Array already dimensioned") {
        return 10;
    }
    if msg.starts_with("Out of DATA") || msg.starts_with("Syntax error in DATA") {
        return if msg.starts_with("Out of DATA") { 4 } else { 2 };
    }
    ERROR_TABLE
        .iter()
        .find(|(_, m)| msg.starts_with(m))
        .map(|(c, _)| *c)
        .unwrap_or(5)
}

/// Code page 437 characters 128-255, so `CHR$` box-drawing and accented
/// characters display as they did in QBasic.
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

fn chr(code: i64) -> Result<String, String> {
    match code {
        0..=127 => Ok(char::from(code as u8).to_string()),
        128..=255 => Ok(CP437_HIGH
            .chars()
            .nth(code as usize - 128)
            .unwrap_or('?')
            .to_string()),
        _ => Err(format!(
            "Illegal function call: CHR$({}) is outside 0-255",
            code
        )),
    }
}

fn asc(c: char) -> i64 {
    if (c as u32) < 128 {
        return c as i64;
    }
    CP437_HIGH
        .chars()
        .position(|h| h == c)
        .map(|p| p as i64 + 128)
        .unwrap_or(c as i64)
}

/// Parses a number the way `VAL` does: leading whitespace is skipped and
/// parsing stops at the first character that cannot continue the number.
fn val(s: &str) -> f64 {
    let t: String = s.chars().filter(|c| *c != ' ' && *c != '\t').collect();
    let upper = t.to_ascii_uppercase();
    for (prefix, radix) in [("&H", 16), ("&O", 8), ("&B", 2)] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            let digits: String = rest.chars().take_while(|c| c.is_digit(radix)).collect();
            return i64::from_str_radix(&digits, radix)
                .map(|n| n as f64)
                .unwrap_or(0.0);
        }
    }
    let bytes = upper.as_bytes();
    let mut end = 0;
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let mut saw_digit = false;
    let mut saw_dot = false;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !saw_dot)) {
        saw_dot |= bytes[i] == b'.';
        saw_digit |= bytes[i].is_ascii_digit();
        i += 1;
        if saw_digit {
            end = i;
        }
    }
    if saw_digit && i < bytes.len() && (bytes[i] == b'E' || bytes[i] == b'D') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            end = j;
        }
    }
    upper[..end].replace('D', "E").parse().unwrap_or(0.0)
}

/// Strict numeric parse used for INPUT and READ: the whole field must be a
/// number (blank counts as 0).
fn parse_number_field(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return Some(0.0);
    }
    let upper = t.to_ascii_uppercase();
    for (prefix, radix) in [("&H", 16), ("&O", 8), ("&B", 2)] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            return i64::from_str_radix(rest, radix).ok().map(|n| n as f64);
        }
    }
    let normalized = upper.replace('D', "E");
    if normalized.contains("INF") || normalized.contains("NAN") {
        return None;
    }
    normalized.parse().ok()
}

/// Splits a keyboard INPUT line into comma-separated fields; double quotes
/// protect commas.
fn split_input_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut was_quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                was_quoted = true;
            }
            ',' if !in_quotes => {
                fields.push(if was_quoted {
                    cur.clone()
                } else {
                    cur.trim().to_string()
                });
                cur.clear();
                was_quoted = false;
            }
            _ => cur.push(ch),
        }
    }
    fields.push(if was_quoted {
        cur
    } else {
        cur.trim().to_string()
    });
    fields
}

fn peek_byte(r: &mut BufReader<File>) -> Result<Option<u8>, String> {
    let buf = r
        .fill_buf()
        .map_err(|e| format!("Device I/O error: {}", e))?;
    Ok(buf.first().copied())
}

/// Reads one comma/newline separated field for `INPUT #`.
fn read_file_field(r: &mut BufReader<File>) -> Result<Option<String>, String> {
    loop {
        match peek_byte(r)? {
            Some(b' ' | b'\t' | b'\r' | b'\n') => r.consume(1),
            Some(_) => break,
            None => return Ok(None),
        }
    }
    let mut bytes = Vec::new();
    if peek_byte(r)? == Some(b'"') {
        r.consume(1);
        while let Some(b) = peek_byte(r)? {
            r.consume(1);
            if b == b'"' {
                break;
            }
            bytes.push(b);
        }
        while let Some(b) = peek_byte(r)? {
            r.consume(1);
            if b == b',' || b == b'\n' {
                break;
            }
        }
        return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
    }
    while let Some(b) = peek_byte(r)? {
        r.consume(1);
        if b == b',' || b == b'\n' {
            break;
        }
        bytes.push(b);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).trim_end().to_string()))
}

fn seed_from_number(seed: f64) -> u64 {
    let mixed = seed.to_bits() ^ 0x9E37_79B9_7F4A_7C15;
    if mixed == 0 {
        0x1234_5678
    } else {
        mixed
    }
}

fn seconds_since_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Civil date (year, month, day) for a count of days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn flatten_index(arr: &ArrayValue, idxs: &[i64]) -> Result<usize, String> {
    if arr.dims.len() != idxs.len() {
        return Err(format!(
            "Subscript out of range: array has {} dimension(s) but {} index(es) were given",
            arr.dims.len(),
            idxs.len()
        ));
    }
    let mut flat = 0usize;
    let mut mult = 1usize;
    for i in (0..arr.dims.len()).rev() {
        let offset = idxs[i] - arr.lower_bounds[i];
        if offset < 0 || offset as usize >= arr.dims[i] {
            return Err(format!(
                "Subscript out of range: index {} is outside {} TO {}",
                idxs[i],
                arr.lower_bounds[i],
                arr.lower_bounds[i] + arr.dims[i] as i64 - 1
            ));
        }
        flat += offset as usize * mult;
        mult *= arr.dims[i];
    }
    Ok(flat)
}

fn unflatten_index(flat: usize, arr: &ArrayValue) -> Vec<i64> {
    let mut remaining = flat;
    let mut idxs = vec![0; arr.dims.len()];
    for i in (0..arr.dims.len()).rev() {
        idxs[i] = arr.lower_bounds[i] + (remaining % arr.dims[i]) as i64;
        remaining /= arr.dims[i];
    }
    idxs
}

fn apply_binop(l: &Value, op: &BinOp, r: &Value) -> Result<Value, String> {
    if let (Value::Str(a), Value::Str(b)) = (l, r) {
        return match op {
            BinOp::Add => {
                if a.len() + b.len() > MAX_STRING_LEN {
                    return Err("Out of string space".to_string());
                }
                Ok(Value::Str(format!("{}{}", a, b)))
            }
            BinOp::Eq => Ok(bool_value(a == b)),
            BinOp::NotEq => Ok(bool_value(a != b)),
            BinOp::Lt => Ok(bool_value(a < b)),
            BinOp::Le => Ok(bool_value(a <= b)),
            BinOp::Gt => Ok(bool_value(a > b)),
            BinOp::Ge => Ok(bool_value(a >= b)),
            _ => Err("Type mismatch: operator cannot be used with strings".to_string()),
        };
    }
    let (a, b) = match (l, r) {
        (Value::Number(a), Value::Number(b)) => (*a, *b),
        _ => return Err("Type mismatch: cannot combine a string and a number".to_string()),
    };
    let finite = |x: f64| {
        if x.is_finite() {
            Ok(Value::Number(x))
        } else if x.is_nan() {
            Err("Illegal function call".to_string())
        } else {
            Err("Overflow".to_string())
        }
    };
    match op {
        BinOp::Add => finite(a + b),
        BinOp::Sub => finite(a - b),
        BinOp::Mul => finite(a * b),
        BinOp::Div => {
            if b == 0.0 {
                return Err("Division by zero".to_string());
            }
            finite(a / b)
        }
        BinOp::IntDiv | BinOp::Mod => {
            let (ia, ib) = (to_int(a), to_int(b));
            if ib == 0 {
                return Err("Division by zero".to_string());
            }
            let r = if *op == BinOp::IntDiv {
                ia.checked_div(ib)
            } else {
                ia.checked_rem(ib)
            };
            r.map(|v| Value::Number(v as f64))
                .ok_or_else(|| "Overflow".to_string())
        }
        BinOp::Pow => {
            if a == 0.0 && b < 0.0 {
                return Err("Division by zero".to_string());
            }
            finite(a.powf(b))
        }
        BinOp::Eq => Ok(bool_value(a == b)),
        BinOp::NotEq => Ok(bool_value(a != b)),
        BinOp::Lt => Ok(bool_value(a < b)),
        BinOp::Le => Ok(bool_value(a <= b)),
        BinOp::Gt => Ok(bool_value(a > b)),
        BinOp::Ge => Ok(bool_value(a >= b)),
        BinOp::And => Ok(Value::Number((to_int(a) & to_int(b)) as f64)),
        BinOp::Or => Ok(Value::Number((to_int(a) | to_int(b)) as f64)),
        BinOp::Xor => Ok(Value::Number((to_int(a) ^ to_int(b)) as f64)),
    }
}

fn block_label(body: &[Stmt], target: &str) -> Option<usize> {
    body.iter().position(|s| match s {
        Stmt::Label(n) => n == target,
        Stmt::LineNumber(n) => n.to_string() == target,
        _ => false,
    })
}

fn advance_col(col: &mut usize, text: &str) {
    match text.rfind('\n') {
        Some(pos) => *col = text[pos + 1..].chars().count(),
        None => *col += text.chars().count(),
    }
}

impl Interpreter {
    fn new(io: Io) -> Self {
        Interpreter {
            frames: Vec::new(),
            subs: HashMap::new(),
            types: HashMap::new(),
            constants: HashMap::new(),
            global_shared: HashSet::new(),
            static_store: HashMap::new(),
            data: Vec::new(),
            data_ptr: 0,
            data_labels: HashMap::new(),
            files: HashMap::new(),
            option_base: 0,
            default_types: Vec::new(),
            rng_state: 0x1234_5678,
            last_rnd: 0.0,
            io,
            cur_line: 1,
            pending_line_number: None,
            basic_line: None,
            last_line_number: 0,
            depth: 0,
            on_error: None,
            in_handler: false,
            err_code: 0,
            err_line: 0,
            cursor_col: 0,
            cursor_row: 1,
        }
    }

    fn located(&self, e: &str) -> String {
        match self.basic_line {
            Some(n) => format!("Line {} (BASIC line {}): {}", self.cur_line, n, e),
            None => format!("Line {}: {}", self.cur_line, e),
        }
    }

    fn run_program(&mut self, stmts: Vec<Stmt>) -> Result<(), String> {
        self.collect_data(&stmts);
        let mut consts = Vec::new();
        collect_consts(&stmts, &mut consts);
        let mut main_body = Vec::new();
        for s in stmts {
            match s {
                Stmt::SubDef {
                    name,
                    params,
                    body,
                    is_static,
                } => {
                    self.register_sub(name, params, body, false, VarType::Auto, is_static);
                }
                Stmt::FuncDef {
                    name,
                    params,
                    body,
                    ret_type,
                    is_static,
                } => {
                    self.register_sub(name, params, body, true, ret_type, is_static);
                }
                Stmt::TypeDef(name, fields) => {
                    self.types.insert(name, fields);
                }
                other => main_body.push(other),
            }
        }
        let main = Routine::new(main_body);
        self.frames.push(Frame::new(Rc::clone(&main), None));
        for (line, name, expr) in consts {
            self.cur_line = line;
            let v = self.eval(&expr)?;
            self.constants.insert(name, v);
        }
        self.cur_line = 1;
        match self.exec_list(&main.body, Some(&main.labels), 0)? {
            Flow::Normal | Flow::End => Ok(()),
            Flow::Return => Err("RETURN without GOSUB".to_string()),
            Flow::Goto(t) => Err(format!("Label {} is not defined", t)),
            Flow::Resume(_) => Err("RESUME without error".to_string()),
            Flow::ExitFor | Flow::ExitDo | Flow::ExitWhile | Flow::ExitSub => Ok(()),
        }
    }

    fn register_sub(
        &mut self,
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        is_function: bool,
        ret_type: VarType,
        is_static: bool,
    ) {
        self.subs.insert(
            name,
            Rc::new(SubInfo {
                params,
                routine: Routine::new(body),
                is_function,
                ret_type,
                is_static,
            }),
        );
    }

    fn collect_data(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            match s {
                Stmt::Data(items) => self.data.extend(items.iter().cloned()),
                Stmt::Label(n) => {
                    self.data_labels.insert(n.clone(), self.data.len());
                }
                Stmt::LineNumber(n) => {
                    self.data_labels.insert(n.to_string(), self.data.len());
                }
                Stmt::SubDef { body, .. } | Stmt::FuncDef { body, .. } => self.collect_data(body),
                Stmt::If {
                    then_branch,
                    elseifs,
                    else_branch,
                    ..
                } => {
                    self.collect_data(then_branch);
                    for (_, b) in elseifs {
                        self.collect_data(b);
                    }
                    self.collect_data(else_branch);
                }
                Stmt::SelectCase {
                    cases, else_branch, ..
                } => {
                    for (_, b) in cases {
                        self.collect_data(b);
                    }
                    self.collect_data(else_branch);
                }
                Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::DoLoop { body, .. } => {
                    self.collect_data(body)
                }
                _ => {}
            }
        }
    }

    // ---- Execution ----------------------------------------------------------

    fn exec_list(
        &mut self,
        body: &[Stmt],
        labels: Option<&HashMap<String, usize>>,
        start: usize,
    ) -> Result<Flow, String> {
        // Checked here too so that empty loops (`DO: LOOP`) can be stopped.
        if self.io.cancel.load(Ordering::Relaxed) {
            return Err(INTERRUPTED.to_string());
        }
        let mut pc = start;
        while pc < body.len() {
            match self.exec_guarded(&body[pc])? {
                Flow::Normal => pc += 1,
                Flow::Goto(t) => {
                    let found = match labels {
                        Some(map) => map.get(&t).copied(),
                        None => block_label(body, &t),
                    };
                    match found {
                        Some(i) => pc = i,
                        None => return Ok(Flow::Goto(t)),
                    }
                }
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_block(&mut self, body: &[Stmt]) -> Result<Flow, String> {
        self.exec_list(body, None, 0)
    }

    /// Executes a statement, diverting runtime errors to an active
    /// `ON ERROR GOTO` handler.
    fn exec_guarded(&mut self, stmt: &Stmt) -> Result<Flow, String> {
        loop {
            match self.exec_stmt(stmt) {
                Err(e)
                    if self.on_error.is_some()
                        && !self.in_handler
                        && self.frames.len() == 1
                        && e != END_SIGNAL
                        && e != INTERRUPTED =>
                {
                    match self.run_error_handler(&e)? {
                        Some(flow) => return Ok(flow),
                        None => continue,
                    }
                }
                other => return other,
            }
        }
    }

    /// Runs the error handler. Returns `None` to retry the failed statement.
    fn run_error_handler(&mut self, e: &str) -> Result<Option<Flow>, String> {
        self.err_code = error_code(e);
        self.err_line = self.last_line_number;
        let target = self.on_error.clone().unwrap_or_default();
        let routine = Rc::clone(&self.frames[0].routine);
        let idx = *routine
            .labels
            .get(&target)
            .ok_or_else(|| format!("Label {} is not defined", target))?;
        let (line, basic) = (self.cur_line, self.basic_line);
        self.in_handler = true;
        let result = self.exec_list(&routine.body, Some(&routine.labels), idx);
        self.in_handler = false;
        let flow = result?;
        let outcome = match flow {
            Flow::Resume(ResumeKind::Retry) => None,
            Flow::Resume(ResumeKind::Next) => Some(Flow::Normal),
            Flow::Resume(ResumeKind::Label(t)) => Some(Flow::Goto(t)),
            Flow::End | Flow::Normal => Some(Flow::End),
            Flow::Return => return Err("RETURN without GOSUB".to_string()),
            Flow::Goto(t) => return Err(format!("Label {} is not defined", t)),
            _ => return Err("No RESUME at end of error handler".to_string()),
        };
        if !matches!(outcome, Some(Flow::End)) {
            self.cur_line = line;
            self.basic_line = basic;
            self.err_code = 0;
        }
        Ok(outcome)
    }

    fn enter_call(&mut self) -> Result<(), String> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(format!(
                "Out of stack space: more than {} nested SUB/FUNCTION calls or GOSUBs",
                MAX_CALL_DEPTH
            ));
        }
        self.depth += 1;
        Ok(())
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Flow, String> {
        if self.io.cancel.load(Ordering::Relaxed) {
            return Err(INTERRUPTED.to_string());
        }
        match stmt {
            Stmt::SrcLine(l) => {
                self.cur_line = *l;
                self.basic_line = self.pending_line_number.take();
                Ok(Flow::Normal)
            }
            Stmt::LineNumber(n) => {
                self.pending_line_number = Some(*n);
                self.last_line_number = *n;
                Ok(Flow::Normal)
            }
            Stmt::Label(_)
            | Stmt::Const(_, _)
            | Stmt::Noop
            | Stmt::TypeDef(_, _)
            | Stmt::Data(_)
            | Stmt::Window
            | Stmt::SubDef { .. }
            | Stmt::FuncDef { .. } => Ok(Flow::Normal),
            Stmt::Let(lv, expr) => {
                let v = self.eval(expr)?;
                let (root, segs) = self.lvalue_path(lv)?;
                self.assign_path(&root, &segs, v)?;
                Ok(Flow::Normal)
            }
            Stmt::If {
                cond,
                then_branch,
                elseifs,
                else_branch,
            } => {
                if is_truthy(&self.eval(cond)?) {
                    return self.exec_block(then_branch);
                }
                for (c, b) in elseifs {
                    if is_truthy(&self.eval(c)?) {
                        return self.exec_block(b);
                    }
                }
                self.exec_block(else_branch)
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
            } => self.exec_for(var, start, end, step.as_ref(), body),
            Stmt::While { cond, body } => {
                while is_truthy(&self.eval(cond)?) {
                    match self.exec_block(body)? {
                        Flow::Normal => {}
                        Flow::ExitWhile => break,
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::DoLoop { cond, body } => self.exec_do(cond.as_ref(), body),
            Stmt::SelectCase {
                expr,
                cases,
                else_branch,
            } => {
                let target = self.eval(expr)?;
                for (clauses, body) in cases {
                    for cl in clauses {
                        if self.case_matches(&target, cl)? {
                            return self.exec_block(body);
                        }
                    }
                }
                self.exec_block(else_branch)
            }
            Stmt::Goto(t) => Ok(Flow::Goto(t.clone())),
            Stmt::Gosub(t) => self.do_gosub(t),
            Stmt::Return => Ok(Flow::Return),
            Stmt::End | Stmt::Stop | Stmt::System => Ok(Flow::End),
            Stmt::Exit(kind) => Ok(match kind.as_str() {
                "FOR" => Flow::ExitFor,
                "DO" => Flow::ExitDo,
                "WHILE" => Flow::ExitWhile,
                _ => Flow::ExitSub,
            }),
            Stmt::OnJump {
                selector,
                targets,
                is_gosub,
            } => {
                let idx = to_int(self.eval(selector)?.as_number()?);
                if !(0..=255).contains(&idx) {
                    return Err("Illegal function call: ON index must be 0-255".to_string());
                }
                if idx == 0 || idx as usize > targets.len() {
                    return Ok(Flow::Normal);
                }
                let target = &targets[idx as usize - 1];
                if *is_gosub {
                    self.do_gosub(target)
                } else {
                    Ok(Flow::Goto(target.clone()))
                }
            }
            Stmt::OnError(target) => {
                self.on_error = target.clone();
                Ok(Flow::Normal)
            }
            Stmt::Resume(kind) => {
                if !self.in_handler {
                    return Err("RESUME without error".to_string());
                }
                Ok(Flow::Resume(kind.clone()))
            }
            Stmt::RaiseError(e) => {
                let code = to_int(self.eval(e)?.as_number()?);
                if !(1..=255).contains(&code) {
                    return Err("Illegal function call: ERROR code must be 1-255".to_string());
                }
                Err(error_message(code))
            }
            Stmt::CallSub(name, args) => {
                self.invoke_sub(name, args)?;
                Ok(Flow::Normal)
            }
            _ => self.exec_simple(stmt),
        }
    }

    /// Statements that never change control flow; kept out of `exec_stmt`
    /// so its stack frame stays small for deep recursion.
    #[inline(never)]
    fn exec_simple(&mut self, stmt: &Stmt) -> Result<Flow, String> {
        match stmt {
            Stmt::Print {
                handle,
                using,
                items,
            } => self.do_print(handle.as_ref(), using.as_ref(), items)?,
            Stmt::Write { handle, items } => self.do_write(handle.as_ref(), items)?,
            Stmt::Input {
                handle,
                prompt,
                question,
                vars,
            } => match handle {
                Some(h) => self.do_file_input(h, vars)?,
                None => self.do_input(prompt.as_deref(), *question, vars)?,
            },
            Stmt::LineInput {
                handle,
                prompt,
                var,
            } => self.do_line_input(handle.as_ref(), prompt.as_deref(), var)?,
            Stmt::MidAssign {
                target,
                start,
                len,
                value,
            } => self.do_mid_assign(target, start, len.as_ref(), value)?,
            Stmt::OptionBase(e) => {
                let base = self.eval(e)?.as_number()?;
                if base != 0.0 && base != 1.0 {
                    return Err("OPTION BASE must be 0 or 1".to_string());
                }
                self.option_base = base as i64;
            }
            Stmt::DefType(vtype, ranges) => {
                for (start, end) in ranges {
                    self.default_types.push((*start, *end, vtype.clone()));
                }
            }
            Stmt::Dim { decls, shared } => {
                for d in decls {
                    self.do_dim(d, *shared)?;
                }
            }
            Stmt::Redim(decls, preserve) => {
                for d in decls {
                    self.do_redim(d, *preserve)?;
                }
            }
            Stmt::Static(decls) => self.do_static(decls)?,
            Stmt::Shared(names) => {
                if self.frames.len() > 1 {
                    let frame = self.frames.last_mut().unwrap();
                    frame.shared.extend(names.iter().cloned());
                }
            }
            Stmt::Erase(names) => {
                for name in names {
                    let si = self.scope_of(name);
                    self.frames[si].vars.remove(name);
                }
            }
            Stmt::Read(vars) => self.do_read(vars)?,
            Stmt::Restore(target) => {
                self.data_ptr = match target {
                    Some(label) => *self
                        .data_labels
                        .get(label)
                        .ok_or_else(|| format!("Label {} is not defined", label))?,
                    None => 0,
                };
            }
            Stmt::Swap(left, right) => {
                let (lr, ls) = self.lvalue_path(left)?;
                let (rr, rs) = self.lvalue_path(right)?;
                let lv = self.read_path(&lr, &ls)?;
                let rv = self.read_path(&rr, &rs)?;
                self.assign_path(&lr, &ls, rv)?;
                self.assign_path(&rr, &rs, lv)?;
            }
            Stmt::Clear => {
                for f in &mut self.frames {
                    f.vars.clear();
                    f.declared.clear();
                }
                self.files.clear();
            }
            Stmt::Randomize(seed) => {
                self.rng_state = match seed {
                    Some(expr) => seed_from_number(self.eval(expr)?.as_number()?),
                    None => seed_from_number(seconds_since_epoch()),
                };
            }
            Stmt::Sleep(arg) => self.do_sleep(arg.as_ref())?,
            Stmt::Beep => self.console_print("\x07".to_string(), false),
            Stmt::Open { .. }
            | Stmt::Close(_)
            | Stmt::Get { .. }
            | Stmt::Put { .. }
            | Stmt::Kill(_) => self.exec_file_stmt(stmt)?,
            _ => self.exec_screen_stmt(stmt)?,
        }
        Ok(Flow::Normal)
    }

    fn exec_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
    ) -> Result<Flow, String> {
        let s = self.eval(start)?.as_number()?;
        let e = self.eval(end)?.as_number()?;
        let st = match step {
            Some(x) => self.eval(x)?.as_number()?,
            None => 1.0,
        };
        if st == 0.0 {
            return Err("Illegal function call: FOR STEP cannot be 0".to_string());
        }
        self.assign_path(var, &[], Value::Number(s))?;
        loop {
            let cur = self.get_var(var)?.as_number()?;
            if (st > 0.0 && cur > e) || (st < 0.0 && cur < e) {
                break;
            }
            match self.exec_block(body)? {
                Flow::Normal => {}
                Flow::ExitFor => break,
                other => return Ok(other),
            }
            let cur = self.get_var(var)?.as_number()?;
            self.assign_path(var, &[], Value::Number(cur + st))?;
        }
        Ok(Flow::Normal)
    }

    fn exec_do(
        &mut self,
        cond: Option<&(Expr, bool, bool)>,
        body: &[Stmt],
    ) -> Result<Flow, String> {
        loop {
            if let Some((c, is_until, true)) = cond {
                if is_truthy(&self.eval(c)?) == *is_until {
                    break;
                }
            }
            match self.exec_block(body)? {
                Flow::Normal => {}
                Flow::ExitDo => break,
                other => return Ok(other),
            }
            if let Some((c, is_until, false)) = cond {
                if is_truthy(&self.eval(c)?) == *is_until {
                    break;
                }
            }
        }
        Ok(Flow::Normal)
    }

    /// Runs a GOSUB subroutine to its RETURN, then continues after the
    /// GOSUB statement — wherever it is nested.
    fn do_gosub(&mut self, target: &str) -> Result<Flow, String> {
        let routine = Rc::clone(&self.frames.last().unwrap().routine);
        let idx = *routine
            .labels
            .get(target)
            .ok_or_else(|| format!("Label {} is not defined", target))?;
        self.enter_call()?;
        let result = self.exec_list(&routine.body, Some(&routine.labels), idx);
        self.depth -= 1;
        Ok(match result? {
            Flow::Return => Flow::Normal,
            Flow::Normal if self.frames.len() > 1 => Flow::ExitSub,
            Flow::Normal => Flow::End,
            other => other,
        })
    }

    fn case_matches(&mut self, target: &Value, cl: &CaseClause) -> Result<bool, String> {
        Ok(match cl {
            CaseClause::Value(e) => {
                let v = self.eval(e)?;
                is_truthy(&apply_binop(target, &BinOp::Eq, &v)?)
            }
            CaseClause::Range(a, b) => {
                let av = self.eval(a)?;
                let bv = self.eval(b)?;
                is_truthy(&apply_binop(target, &BinOp::Ge, &av)?)
                    && is_truthy(&apply_binop(target, &BinOp::Le, &bv)?)
            }
            CaseClause::Compare(op, e) => {
                let v = self.eval(e)?;
                is_truthy(&apply_binop(target, op, &v)?)
            }
        })
    }

    // ---- Variables ----------------------------------------------------------

    /// Index of the frame that owns `name`: locals and parameters first, then
    /// SHARED globals.
    fn scope_of(&self, name: &str) -> usize {
        let top = self.frames.len() - 1;
        if top == 0 {
            return 0;
        }
        let f = &self.frames[top];
        if f.vars.contains_key(name) || f.declared.contains_key(name) {
            return top;
        }
        if f.shared.contains(name) || self.global_shared.contains(name) {
            return 0;
        }
        top
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.frames[self.scope_of(name)].vars.get(name)
    }

    fn scalar_type(&self, name: &str) -> VarType {
        self.frames[self.scope_of(name)]
            .declared
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_type_with_defaults(name, &self.default_types))
    }

    fn get_var(&mut self, name: &str) -> Result<Value, String> {
        if let Some(v) = self.constants.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.lookup(name) {
            return Ok(v.clone());
        }
        let t = self.scalar_type(name);
        self.default_value(&t)
    }

    fn default_value(&self, vt: &VarType) -> Result<Value, String> {
        Ok(match vt {
            VarType::Str => Value::Str(String::new()),
            VarType::FixedStr(n) => Value::Str(" ".repeat(*n)),
            VarType::UserType(tn) => {
                let fields = self
                    .types
                    .get(tn)
                    .ok_or_else(|| format!("Unknown type '{}'", tn))?;
                let mut out = Vec::with_capacity(fields.len());
                for (n, t) in fields {
                    out.push((n.clone(), self.default_value(t)?));
                }
                Value::Record(RecordValue {
                    type_name: tn.clone(),
                    fields: out,
                })
            }
            _ => Value::Number(0.0),
        })
    }

    /// Converts a value for storage in a variable of type `vt`.
    fn coerce(&self, v: Value, vt: &VarType) -> Result<Value, String> {
        match (vt, v) {
            (VarType::Str, Value::Str(s)) => Ok(Value::Str(s)),
            (VarType::FixedStr(n), Value::Str(s)) => {
                let mut s: String = s.chars().take(*n).collect();
                let len = s.chars().count();
                s.extend(std::iter::repeat_n(' ', n - len));
                Ok(Value::Str(s))
            }
            (VarType::Integer, Value::Number(x)) => {
                let r = x.round_ties_even();
                if !(-32768.0..=32767.0).contains(&r) {
                    return Err(format!(
                        "Overflow: {} does not fit in an INTEGER",
                        format_number(x)
                    ));
                }
                Ok(Value::Number(r))
            }
            (VarType::Long, Value::Number(x)) => {
                let r = x.round_ties_even();
                if !(-2147483648.0..=2147483647.0).contains(&r) {
                    return Err(format!(
                        "Overflow: {} does not fit in a LONG",
                        format_number(x)
                    ));
                }
                Ok(Value::Number(r))
            }
            (VarType::Single | VarType::Double | VarType::Auto, Value::Number(x)) => {
                Ok(Value::Number(x))
            }
            (VarType::UserType(t), Value::Record(r)) => {
                if &r.type_name != t {
                    return Err(format!(
                        "Type mismatch: cannot assign a {} record to a {} variable",
                        r.type_name, t
                    ));
                }
                Ok(Value::Record(r))
            }
            (_, Value::Array(a)) => Ok(Value::Array(a)),
            (VarType::Auto, v) => Ok(v),
            (vt, Value::Number(_)) if vt.is_string() => {
                Err("Type mismatch: cannot store a number in a string variable".to_string())
            }
            (_, Value::Str(_)) => {
                Err("Type mismatch: cannot store a string in a numeric variable".to_string())
            }
            _ => Err("Type mismatch".to_string()),
        }
    }

    fn lvalue_path(&mut self, lv: &LValue) -> Result<(String, Vec<Seg>), String> {
        match lv {
            LValue::Var(n) => Ok((n.clone(), Vec::new())),
            LValue::Index(n, exprs) => {
                let mut idxs = Vec::with_capacity(exprs.len());
                for e in exprs {
                    idxs.push(to_int(self.eval(e)?.as_number()?));
                }
                Ok((n.clone(), vec![Seg::Idx(idxs)]))
            }
            LValue::Field(parent, f) => {
                let (root, mut segs) = self.lvalue_path(parent)?;
                segs.push(Seg::Field(f.clone()));
                Ok((root, segs))
            }
        }
    }

    fn path_type(&self, root: &str, segs: &[Seg]) -> VarType {
        let mut t = match (segs.first(), self.lookup(root)) {
            (Some(Seg::Idx(_)), Some(Value::Array(a))) => a.element_type.clone(),
            _ => self.scalar_type(root),
        };
        for seg in segs {
            if let Seg::Field(f) = seg {
                t = match &t {
                    VarType::UserType(tn) => self
                        .types
                        .get(tn)
                        .and_then(|fs| fs.iter().find(|(n, _)| n == f))
                        .map(|(_, ft)| ft.clone())
                        .unwrap_or(VarType::Auto),
                    _ => VarType::Auto,
                };
            }
        }
        t
    }

    fn new_array(
        &self,
        bounds: Vec<(i64, i64)>,
        element_type: VarType,
    ) -> Result<ArrayValue, String> {
        let mut dims = Vec::with_capacity(bounds.len());
        let mut lower_bounds = Vec::with_capacity(bounds.len());
        let mut total: usize = 1;
        for (lo, hi) in bounds {
            if hi < lo {
                return Err(format!(
                    "Subscript out of range: upper bound {} is below lower bound {}",
                    hi, lo
                ));
            }
            let size = usize::try_from(hi - lo + 1).map_err(|_| "Out of memory".to_string())?;
            total = total
                .checked_mul(size)
                .filter(|t| *t <= MAX_ARRAY_ELEMENTS)
                .ok_or_else(|| {
                    format!(
                        "Out of memory: arrays are limited to {} elements",
                        MAX_ARRAY_ELEMENTS
                    )
                })?;
            dims.push(size);
            lower_bounds.push(lo);
        }
        let init = self.default_value(&element_type)?;
        Ok(ArrayValue {
            dims,
            lower_bounds,
            data: vec![init; total],
            element_type,
        })
    }

    /// Mutable access to the storage behind a path, creating the variable
    /// (or an implicit 0-10 array, as QBasic does) if it does not exist.
    fn slot_mut(&mut self, root: &str, segs: &[Seg]) -> Result<&mut Value, String> {
        let si = self.scope_of(root);
        if !self.frames[si].vars.contains_key(root) {
            let t = self.scalar_type(root);
            let init = match segs.first() {
                Some(Seg::Idx(ix)) => {
                    let bounds = vec![(self.option_base, 10); ix.len()];
                    Value::Array(self.new_array(bounds, t)?)
                }
                _ => self.default_value(&t)?,
            };
            self.frames[si].vars.insert(root.to_string(), init);
        }
        let mut cur = self.frames[si].vars.get_mut(root).unwrap();
        for seg in segs {
            cur = match seg {
                Seg::Idx(ix) => match cur {
                    Value::Array(a) => {
                        let flat = flatten_index(a, ix)?;
                        &mut a.data[flat]
                    }
                    _ => return Err(format!("'{}' is not an array", root)),
                },
                Seg::Field(f) => match cur {
                    Value::Record(r) => r
                        .fields
                        .iter_mut()
                        .find(|(n, _)| n == f)
                        .map(|(_, v)| v)
                        .ok_or_else(|| format!("Field '{}' not found", f))?,
                    _ => return Err(format!("'{}' is not a record", root)),
                },
            };
        }
        Ok(cur)
    }

    fn read_path(&mut self, root: &str, segs: &[Seg]) -> Result<Value, String> {
        if segs.is_empty() {
            return self.get_var(root);
        }
        Ok(self.slot_mut(root, segs)?.clone())
    }

    fn assign_path(&mut self, root: &str, segs: &[Seg], v: Value) -> Result<(), String> {
        if segs.is_empty() && self.constants.contains_key(root) {
            return Err(format!("Cannot assign to constant '{}'", root));
        }
        let t = self.path_type(root, segs);
        let coerced = self.coerce(v, &t)?;
        *self.slot_mut(root, segs)? = coerced;
        Ok(())
    }

    fn bounds_for(&mut self, d: &DimDecl) -> Result<Vec<(i64, i64)>, String> {
        let mut bounds = Vec::with_capacity(d.dims.len());
        for (lo, hi) in &d.dims {
            let lower = match lo {
                Some(e) => to_int(self.eval(e)?.as_number()?),
                None => self.option_base,
            };
            let upper = to_int(self.eval(hi)?.as_number()?);
            bounds.push((lower, upper));
        }
        Ok(bounds)
    }

    fn do_dim(&mut self, d: &DimDecl, shared: bool) -> Result<(), String> {
        let si = if shared { 0 } else { self.frames.len() - 1 };
        if shared {
            self.global_shared.insert(d.name.clone());
        }
        self.frames[si]
            .declared
            .insert(d.name.clone(), d.vtype.clone());
        if d.dims.is_empty() {
            if !d.is_array && !self.frames[si].vars.contains_key(&d.name) {
                let init = self.default_value(&d.vtype)?;
                self.frames[si].vars.insert(d.name.clone(), init);
            }
            return Ok(());
        }
        if matches!(self.frames[si].vars.get(&d.name), Some(Value::Array(_))) {
            return Err(format!("Array already dimensioned: {}", d.name));
        }
        let bounds = self.bounds_for(d)?;
        let arr = self.new_array(bounds, d.vtype.clone())?;
        self.frames[si]
            .vars
            .insert(d.name.clone(), Value::Array(arr));
        Ok(())
    }

    fn do_redim(&mut self, d: &DimDecl, preserve: bool) -> Result<(), String> {
        let si = self.scope_of(&d.name);
        let bounds = self.bounds_for(d)?;
        let mut new_arr = self.new_array(bounds, d.vtype.clone())?;
        if preserve {
            if let Some(Value::Array(old)) = self.frames[si].vars.get(&d.name) {
                for (flat, value) in old.data.iter().enumerate() {
                    let idxs = unflatten_index(flat, old);
                    if let Ok(nf) = flatten_index(&new_arr, &idxs) {
                        new_arr.data[nf] = value.clone();
                    }
                }
            }
        }
        self.frames[si]
            .declared
            .insert(d.name.clone(), d.vtype.clone());
        self.frames[si]
            .vars
            .insert(d.name.clone(), Value::Array(new_arr));
        Ok(())
    }

    fn do_static(&mut self, decls: &[DimDecl]) -> Result<(), String> {
        let Some(sub) = self.frames.last().unwrap().sub_name.clone() else {
            for d in decls {
                self.do_dim(d, false)?;
            }
            return Ok(());
        };
        for d in decls {
            let saved = self
                .static_store
                .get(&sub)
                .and_then(|m| m.get(&d.name))
                .cloned();
            let top = self.frames.len() - 1;
            self.frames[top].statics.insert(d.name.clone());
            match saved {
                Some(v) => {
                    self.frames[top]
                        .declared
                        .insert(d.name.clone(), d.vtype.clone());
                    self.frames[top].vars.insert(d.name.clone(), v);
                }
                None => {
                    self.frames[top].vars.remove(&d.name);
                    self.do_dim(d, false)?;
                }
            }
        }
        Ok(())
    }

    // ---- Expressions --------------------------------------------------------

    fn eval(&mut self, e: &Expr) -> Result<Value, String> {
        match e {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::StringLit(s) => Ok(Value::Str(s.clone())),
            Expr::Variable(n) => self.eval_variable(n),
            Expr::FieldAccess(..) => {
                if let Some(lv) = self.expr_as_lvalue(e) {
                    let (root, segs) = self.lvalue_path(&lv)?;
                    return self.read_path(&root, &segs);
                }
                let Expr::FieldAccess(parent, f) = e else {
                    unreachable!()
                };
                match self.eval(parent)? {
                    Value::Record(r) => r
                        .fields
                        .into_iter()
                        .find(|(n, _)| n == f)
                        .map(|(_, v)| v)
                        .ok_or_else(|| format!("Field '{}' not found", f)),
                    _ => Err("Type mismatch: '.' used on a value that is not a record".to_string()),
                }
            }
            Expr::UnaryMinus(inner) => Ok(Value::Number(-self.eval(inner)?.as_number()?)),
            Expr::Not(inner) => {
                let v = self.eval(inner)?.as_number()?;
                Ok(Value::Number(!to_int(v) as f64))
            }
            Expr::BinaryOp(l, op, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                apply_binop(&lv, op, &rv)
            }
            Expr::ArrayOrCall(name, args) => {
                if is_builtin(name) {
                    return self.call_builtin(name, args);
                }
                if self.subs.contains_key(name) {
                    return self.invoke_sub(name, args);
                }
                let mut idxs = Vec::with_capacity(args.len());
                for a in args {
                    idxs.push(to_int(self.eval(a)?.as_number()?));
                }
                self.read_path(name, &[Seg::Idx(idxs)])
            }
        }
    }

    fn eval_variable(&mut self, n: &str) -> Result<Value, String> {
        if ZERO_ARG_BUILTINS.contains(&n) {
            return self.call_builtin(n, &[]);
        }
        if let Some(v) = self.constants.get(n) {
            return Ok(v.clone());
        }
        if let Some(v) = self.lookup(n) {
            return Ok(v.clone());
        }
        if let Some(info) = self.subs.get(n) {
            if info.is_function && info.params.is_empty() {
                return self.invoke_sub(n, &[]);
            }
        }
        let t = self.scalar_type(n);
        self.default_value(&t)
    }

    /// The assignable location an argument expression refers to, if any.
    fn expr_as_lvalue(&self, e: &Expr) -> Option<LValue> {
        match e {
            Expr::Variable(n) => {
                if self.constants.contains_key(n)
                    || ZERO_ARG_BUILTINS.contains(&n.as_str())
                    || (self.subs.contains_key(n) && self.lookup(n).is_none())
                {
                    None
                } else {
                    Some(LValue::Var(n.clone()))
                }
            }
            Expr::ArrayOrCall(n, args) => {
                if is_builtin(n) || self.subs.contains_key(n) || args.is_empty() {
                    None
                } else {
                    Some(LValue::Index(n.clone(), args.clone()))
                }
            }
            Expr::FieldAccess(parent, f) => self
                .expr_as_lvalue(parent)
                .map(|p| LValue::Field(Box::new(p), f.clone())),
            _ => None,
        }
    }

    fn invoke_sub(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        let info = Rc::clone(
            self.subs
                .get(name)
                .ok_or_else(|| format!("Unknown SUB or FUNCTION '{}'", name))?,
        );
        if args.len() != info.params.len() {
            return Err(format!(
                "{} expects {} argument(s) but got {}",
                name,
                info.params.len(),
                args.len()
            ));
        }
        let mut values = Vec::with_capacity(args.len());
        let mut refs: Vec<Option<(String, Vec<Seg>)>> = Vec::with_capacity(args.len());
        for (p, a) in info.params.iter().zip(args) {
            if p.is_array {
                let arr_name = match a {
                    Expr::Variable(n) => n.clone(),
                    Expr::ArrayOrCall(n, a) if a.is_empty() => n.clone(),
                    _ => return Err(format!("Parameter {} expects an array, like A()", p.name)),
                };
                match self.lookup(&arr_name) {
                    Some(v @ Value::Array(_)) => values.push(v.clone()),
                    _ => {
                        return Err(format!(
                            "Parameter {} expects an array but {} is not one",
                            p.name, arr_name
                        ))
                    }
                }
                refs.push(p.by_ref.then(|| (arr_name, Vec::new())));
                continue;
            }
            if p.by_ref {
                if let Some(lv) = self.expr_as_lvalue(a) {
                    let (root, segs) = self.lvalue_path(&lv)?;
                    values.push(self.read_path(&root, &segs)?);
                    refs.push(Some((root, segs)));
                    continue;
                }
            }
            values.push(self.eval(a)?);
            refs.push(None);
        }
        self.enter_call()?;
        let mut frame = Frame::new(Rc::clone(&info.routine), Some(name.to_string()));
        for (p, v) in info.params.iter().zip(values) {
            let v = if p.is_array {
                v
            } else {
                self.coerce(v, &p.vtype)
                    .map_err(|e| format!("{} (argument {} of {})", e, p.name, name))?
            };
            frame.vars.insert(p.name.clone(), v);
            frame.declared.insert(p.name.clone(), p.vtype.clone());
        }
        if info.is_function {
            frame
                .vars
                .insert(name.to_string(), self.default_value(&info.ret_type)?);
            frame
                .declared
                .insert(name.to_string(), info.ret_type.clone());
        }
        if info.is_static {
            if let Some(saved) = self.static_store.get(name) {
                for (k, v) in saved {
                    frame.vars.insert(k.clone(), v.clone());
                }
            }
        }
        self.frames.push(frame);
        let result = self.exec_list(&info.routine.body, Some(&info.routine.labels), 0);
        let frame = self.frames.pop().unwrap();
        self.depth -= 1;
        let keep: Vec<String> = if info.is_static {
            frame
                .vars
                .keys()
                .filter(|k| *k != name && !info.params.iter().any(|p| &p.name == *k))
                .cloned()
                .collect()
        } else {
            frame.statics.iter().cloned().collect()
        };
        if !keep.is_empty() {
            let store = self.static_store.entry(name.to_string()).or_default();
            for k in keep {
                if let Some(v) = frame.vars.get(&k) {
                    store.insert(k, v.clone());
                }
            }
        }
        match result? {
            Flow::Normal | Flow::ExitSub => {}
            Flow::End => return Err(END_SIGNAL.to_string()),
            Flow::Return => return Err("RETURN without GOSUB".to_string()),
            Flow::Goto(t) => return Err(format!("Label {} is not defined in {}", t, name)),
            Flow::Resume(_) => return Err("RESUME without error".to_string()),
            Flow::ExitFor | Flow::ExitDo | Flow::ExitWhile => {}
        }
        for (p, target) in info.params.iter().zip(refs) {
            if let Some((root, segs)) = target {
                if let Some(v) = frame.vars.get(&p.name) {
                    self.assign_path(&root, &segs, v.clone())?;
                }
            }
        }
        Ok(if info.is_function {
            frame.vars.get(name).cloned().unwrap_or(Value::Number(0.0))
        } else {
            Value::Number(0.0)
        })
    }

    // ---- Built-in functions -------------------------------------------------

    fn array_arg(&self, fname: &str, args: &[Expr]) -> Result<&ArrayValue, String> {
        let name = match args.first() {
            Some(Expr::Variable(n)) => n,
            Some(Expr::ArrayOrCall(n, a)) if a.is_empty() => n,
            _ => return Err(format!("{} expects an array name", fname)),
        };
        match self.lookup(name) {
            Some(Value::Array(a)) => Ok(a),
            _ => Err(format!(
                "Subscript out of range: {} is not a dimensioned array",
                name
            )),
        }
    }

    fn file_number(&mut self, e: &Expr) -> Result<i64, String> {
        let n = to_int(self.eval(e)?.as_number()?);
        if !(1..=255).contains(&n) {
            return Err(format!("Bad file name or number: #{}", n));
        }
        Ok(n)
    }

    fn call_builtin(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        // Functions that inspect their arguments instead of evaluating them.
        match name {
            "UBOUND" | "LBOUND" => {
                let dim = match args.get(1) {
                    Some(e) => to_int(self.eval(e)?.as_number()?),
                    None => 1,
                };
                let arr = self.array_arg(name, args)?;
                if dim < 1 || dim as usize > arr.dims.len() {
                    return Err(format!("Subscript out of range: dimension {}", dim));
                }
                let i = dim as usize - 1;
                let lo = arr.lower_bounds[i];
                return Ok(Value::Number(if name == "LBOUND" {
                    lo as f64
                } else {
                    (lo + arr.dims[i] as i64 - 1) as f64
                }));
            }
            "LEN" => {
                if let Some(Expr::Variable(n)) = args.first() {
                    let t = self.scalar_type(n);
                    if !t.is_string() && !matches!(t, VarType::UserType(_)) {
                        return Ok(Value::Number(self.type_size(&t) as f64));
                    }
                }
            }
            "EOF" | "LOF" | "INPUT$" => return self.file_builtin(name, args),
            _ => {}
        }
        let mut av = Vec::with_capacity(args.len());
        for a in args {
            av.push(self.eval(a)?);
        }
        let arg = |i: usize| -> Result<&Value, String> {
            av.get(i)
                .ok_or_else(|| format!("{}: missing argument {}", name, i + 1))
        };
        let num = |i: usize| -> Result<f64, String> { arg(i)?.as_number() };
        let string = |i: usize| -> Result<String, String> { arg(i)?.as_string() };
        let count = |i: usize| -> Result<usize, String> {
            let n = to_int(num(i)?);
            if !(0..=32767).contains(&n) {
                return Err(format!(
                    "Illegal function call: {} count must be 0-32767",
                    name
                ));
            }
            Ok(n as usize)
        };
        let illegal = || format!("Illegal function call in {}", name);
        let v = match name {
            "LEN" => match arg(0)? {
                Value::Str(s) => Value::Number(s.chars().count() as f64),
                Value::Record(r) => {
                    Value::Number(self.type_size(&VarType::UserType(r.type_name.clone())) as f64)
                }
                _ => Value::Number(8.0),
            },
            "MID$" => {
                let s = string(0)?;
                let start = to_int(num(1)?);
                if start < 1 {
                    return Err(illegal());
                }
                let iter = s.chars().skip(start as usize - 1);
                Value::Str(if av.len() > 2 {
                    iter.take(count(2)?).collect()
                } else {
                    iter.collect()
                })
            }
            "LEFT$" => Value::Str(string(0)?.chars().take(count(1)?).collect()),
            "RIGHT$" => {
                let s = string(0)?;
                let n = count(1)?;
                let len = s.chars().count();
                Value::Str(s.chars().skip(len.saturating_sub(n)).collect())
            }
            "INT" => Value::Number(num(0)?.floor()),
            "FIX" => Value::Number(num(0)?.trunc()),
            "ABS" => Value::Number(num(0)?.abs()),
            "SQR" => {
                let n = num(0)?;
                if n < 0.0 {
                    return Err(format!(
                        "Illegal function call: SQR of negative number {}",
                        format_number(n)
                    ));
                }
                Value::Number(n.sqrt())
            }
            "RND" => {
                let n = if av.is_empty() { 1.0 } else { num(0)? };
                if n == 0.0 {
                    return Ok(Value::Number(self.last_rnd));
                }
                if n < 0.0 {
                    self.rng_state = seed_from_number(n);
                }
                let mut x = self.rng_state;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                self.rng_state = x;
                self.last_rnd = (x >> 11) as f64 / (1u64 << 53) as f64;
                Value::Number(self.last_rnd)
            }
            "STR$" => {
                let n = num(0)?;
                let s = format_number(n);
                Value::Str(if n >= 0.0 { format!(" {}", s) } else { s })
            }
            "CSTR$" => Value::Str(format_number(num(0)?)),
            "CINT" => self.coerce(Value::Number(num(0)?), &VarType::Integer)?,
            "CLNG" => self.coerce(Value::Number(num(0)?), &VarType::Long)?,
            "CSNG" => Value::Number((num(0)? as f32) as f64),
            "CDBL" => Value::Number(num(0)?),
            "VAL" => Value::Number(val(&string(0)?)),
            "CHR$" => Value::Str(chr(to_int(num(0)?))?),
            "ASC" => {
                let s = string(0)?;
                let c = s
                    .chars()
                    .next()
                    .ok_or_else(|| "Illegal function call: ASC of an empty string".to_string())?;
                Value::Number(asc(c) as f64)
            }
            "UCASE$" => Value::Str(string(0)?.to_uppercase()),
            "LCASE$" => Value::Str(string(0)?.to_lowercase()),
            "LTRIM$" => Value::Str(string(0)?.trim_start_matches(' ').to_string()),
            "RTRIM$" => Value::Str(string(0)?.trim_end_matches(' ').to_string()),
            "TRIM$" => Value::Str(string(0)?.trim_matches(' ').to_string()),
            "INSTR" => {
                let (start, h, n) = if av.len() >= 3 {
                    (to_int(num(0)?), string(1)?, string(2)?)
                } else {
                    (1, string(0)?, string(1)?)
                };
                if start < 1 {
                    return Err(illegal());
                }
                let hay: Vec<char> = h.chars().collect();
                let needle: Vec<char> = n.chars().collect();
                let from = start as usize - 1;
                let found = if from > hay.len() {
                    None
                } else if needle.is_empty() {
                    Some(from)
                } else {
                    (from..=hay.len().saturating_sub(needle.len())).find(|&i| {
                        i + needle.len() <= hay.len() && hay[i..i + needle.len()] == needle[..]
                    })
                };
                Value::Number(found.map(|i| (i + 1) as f64).unwrap_or(0.0))
            }
            "SIN" => Value::Number(num(0)?.sin()),
            "COS" => Value::Number(num(0)?.cos()),
            "TAN" => Value::Number(num(0)?.tan()),
            "ATN" => Value::Number(num(0)?.atan()),
            "LOG" => {
                let n = num(0)?;
                if n <= 0.0 {
                    return Err(format!(
                        "Illegal function call: LOG of {}",
                        format_number(n)
                    ));
                }
                Value::Number(n.ln())
            }
            "EXP" => {
                let r = num(0)?.exp();
                if !r.is_finite() {
                    return Err("Overflow".to_string());
                }
                Value::Number(r)
            }
            "SGN" => Value::Number(num(0)?.signum() * f64::from(num(0)? != 0.0)),
            "TIMER" => Value::Number(seconds_since_epoch() % 86400.0),
            "SPACE$" | "SPC" | "TAB" => Value::Str(" ".repeat(count(0)?)),
            "STRING$" => {
                let n = count(0)?;
                let c = match arg(1)? {
                    Value::Number(code) => chr(to_int(*code))?,
                    Value::Str(s) => s
                        .chars()
                        .next()
                        .ok_or_else(|| {
                            "Illegal function call: STRING$ of an empty string".to_string()
                        })?
                        .to_string(),
                    _ => return Err(illegal()),
                };
                Value::Str(c.repeat(n))
            }
            "HEX$" | "OCT$" => {
                let mut v = to_int(num(0)?);
                if v < 0 {
                    v &= if v >= -32768 { 0xFFFF } else { 0xFFFF_FFFF };
                }
                Value::Str(if name == "HEX$" {
                    format!("{:X}", v)
                } else {
                    format!("{:o}", v)
                })
            }
            "INKEY$" => {
                let key = self.io.keys.lock().ok().and_then(|mut k| k.pop_front());
                if key.is_none() {
                    std::thread::yield_now();
                }
                Value::Str(key.unwrap_or_default())
            }
            "DATE$" | "TIME$" => {
                let secs = seconds_since_epoch() as i64;
                Value::Str(if name == "DATE$" {
                    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
                    format!("{:02}-{:02}-{:04}", m, d, y)
                } else {
                    let t = secs.rem_euclid(86400);
                    format!("{:02}:{:02}:{:02}", t / 3600, (t / 60) % 60, t % 60)
                })
            }
            "ERR" => Value::Number(self.err_code as f64),
            "ERL" => Value::Number(self.err_line as f64),
            "FREEFILE" => Value::Number(
                (1..=255)
                    .find(|n| !self.files.contains_key(n))
                    .unwrap_or(256) as f64,
            ),
            "CSRLIN" => Value::Number(self.cursor_row as f64),
            "POS" => Value::Number((self.cursor_col + 1) as f64),
            _ => return Err(format!("Unknown function {}", name)),
        };
        Ok(v)
    }

    fn file_builtin(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        if name == "INPUT$" {
            let n = match args.first() {
                Some(e) => to_int(self.eval(e)?.as_number()?),
                None => return Err("INPUT$: missing argument 1".to_string()),
            };
            if !(1..=32767).contains(&n) {
                return Err("Illegal function call: INPUT$ count must be 1-32767".to_string());
            }
            if let Some(h) = args.get(1) {
                let h = self.file_number(h)?;
                let Some(FileHandle::Input(r)) = self.files.get_mut(&h) else {
                    return Err(format!("Bad file mode: file #{} is not open for INPUT", h));
                };
                let mut buf = vec![0u8; n as usize];
                r.read_exact(&mut buf)
                    .map_err(|_| "Input past end of file".to_string())?;
                return Ok(Value::Str(String::from_utf8_lossy(&buf).into_owned()));
            }
            let mut out = String::new();
            while out.chars().count() < n as usize {
                if self.io.interactive {
                    if let Some(k) = self.io.keys.lock().ok().and_then(|mut q| q.pop_front()) {
                        out.push_str(&k);
                        continue;
                    }
                    if self.io.cancel.load(Ordering::Relaxed) {
                        return Err(INTERRUPTED.to_string());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                } else {
                    let line = self.read_console_line(String::new())?;
                    out.push_str(&line);
                    if out.chars().count() < n as usize {
                        out.push('\n');
                    }
                }
            }
            return Ok(Value::Str(out.chars().take(n as usize).collect()));
        }
        let Some(h) = args.first() else {
            return Err(format!("{}: missing argument 1", name));
        };
        let h = self.file_number(h)?;
        let handle = self
            .files
            .get_mut(&h)
            .ok_or_else(|| format!("Bad file name or number: file #{} is not open", h))?;
        let v = match (name, handle) {
            ("EOF", FileHandle::Input(r)) => bool_value(peek_byte(r)?.is_none()),
            ("EOF", FileHandle::Random { file, .. }) => {
                let pos = file.stream_position().map_err(|e| e.to_string())?;
                let len = file.metadata().map_err(|e| e.to_string())?.len();
                bool_value(pos >= len)
            }
            ("EOF", FileHandle::Output { .. }) => {
                return Err(format!("Bad file mode: file #{} is open for output", h))
            }
            (_, FileHandle::Input(r)) => {
                Value::Number(r.get_ref().metadata().map_err(|e| e.to_string())?.len() as f64)
            }
            (_, FileHandle::Output { writer, .. }) => {
                writer.flush().map_err(|e| e.to_string())?;
                Value::Number(
                    writer
                        .get_ref()
                        .metadata()
                        .map_err(|e| e.to_string())?
                        .len() as f64,
                )
            }
            (_, FileHandle::Random { file, .. }) => {
                Value::Number(file.metadata().map_err(|e| e.to_string())?.len() as f64)
            }
        };
        Ok(v)
    }

    // ---- Console and file output ------------------------------------------

    fn console_print(&mut self, text: String, newline: bool) {
        advance_col(&mut self.cursor_col, &text);
        self.cursor_row += text.matches('\n').count();
        if newline {
            self.cursor_col = 0;
            self.cursor_row += 1;
        }
        let _ = self.io.console_tx.send(ConsoleMsg::Print { text, newline });
    }

    fn file_writer(&mut self, h: i64) -> Result<(&mut BufWriter<File>, &mut usize), String> {
        match self.files.get_mut(&h) {
            Some(FileHandle::Output { writer, col }) => Ok((writer, col)),
            Some(_) => Err(format!(
                "Bad file mode: file #{} is not open for OUTPUT or APPEND",
                h
            )),
            None => Err(format!("Bad file name or number: file #{} is not open", h)),
        }
    }

    fn emit(&mut self, handle: Option<i64>, text: String, newline: bool) -> Result<(), String> {
        match handle {
            None => {
                self.console_print(text, newline);
                Ok(())
            }
            Some(h) => {
                let (w, col) = self.file_writer(h)?;
                w.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
                advance_col(col, &text);
                if newline {
                    w.write_all(b"\r\n").map_err(|e| e.to_string())?;
                    *col = 0;
                }
                Ok(())
            }
        }
    }

    fn do_print(
        &mut self,
        handle: Option<&Expr>,
        using: Option<&Expr>,
        items: &[PrintItem],
    ) -> Result<(), String> {
        let h = match handle {
            Some(e) => Some(self.file_number(e)?),
            None => None,
        };
        let mut col = match h {
            Some(h) => *self.file_writer(h)?.1,
            None => self.cursor_col,
        };
        let newline = !matches!(items.last(), Some(PrintItem::Semicolon | PrintItem::Comma));
        let mut out = String::new();
        if let Some(fmt) = using {
            let fmt = self.eval(fmt)?.as_string()?;
            let mut args = Vec::new();
            for it in items {
                if let PrintItem::Expr(e) = it {
                    args.push(match self.eval(e)? {
                        Value::Number(n) => UsingArg::Num(n),
                        Value::Str(s) => UsingArg::Str(s),
                        _ => return Err("Type mismatch in PRINT USING".to_string()),
                    });
                }
            }
            out = print_using(&fmt, &args)?;
        } else {
            for it in items {
                match it {
                    PrintItem::Expr(Expr::ArrayOrCall(f, args))
                        if f == "TAB" && args.len() == 1 =>
                    {
                        let target =
                            (to_int(self.eval(&args[0])?.as_number()?).max(1) - 1) as usize;
                        if target < col {
                            out.push('\n');
                            col = 0;
                        }
                        let pad = " ".repeat(target - col);
                        advance_col(&mut col, &pad);
                        out.push_str(&pad);
                    }
                    PrintItem::Expr(e) => {
                        let text = self.eval(e)?.to_display()?;
                        advance_col(&mut col, &text);
                        out.push_str(&text);
                    }
                    PrintItem::Semicolon => {}
                    PrintItem::Comma => {
                        let pad = " ".repeat(ZONE_WIDTH - col % ZONE_WIDTH);
                        advance_col(&mut col, &pad);
                        out.push_str(&pad);
                    }
                }
            }
        }
        self.emit(h, out, newline)
    }

    fn do_write(&mut self, handle: Option<&Expr>, items: &[Expr]) -> Result<(), String> {
        let h = match handle {
            Some(e) => Some(self.file_number(e)?),
            None => None,
        };
        let mut parts = Vec::with_capacity(items.len());
        for e in items {
            parts.push(match self.eval(e)? {
                Value::Str(s) => format!("\"{}\"", s),
                v => v.to_display()?,
            });
        }
        self.emit(h, parts.join(","), true)
    }

    fn exec_screen_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        let num = |s: &mut Self, e: &Expr| -> Result<f64, String> { s.eval(e)?.as_number() };
        let coord = |s: &mut Self, e: &Expr| -> Result<i64, String> { Ok(to_int(num(s, e)?)) };
        let color = |s: &mut Self, e: &Option<Expr>| -> Result<Option<u32>, String> {
            match e {
                Some(e) => {
                    let c = to_int(num(s, e)?);
                    if c < 0 {
                        return Err(format!("Illegal function call: colour {}", c));
                    }
                    Ok(Some(qb_color(c.min(u32::MAX as i64) as u32)))
                }
                None => Ok(None),
            }
        };
        let graphics = Arc::clone(&self.io.graphics);
        let lock = || {
            graphics
                .lock()
                .map_err(|_| "Graphics state unavailable".to_string())
        };
        let need_graphics = |g: &SharedGraphics| {
            if g.active {
                Ok(())
            } else {
                Err("Illegal function call: use SCREEN 1, 7, 9, 12 or 13 before graphics statements".to_string())
            }
        };
        match stmt {
            Stmt::Screen(e) => {
                let mode = to_int(num(self, e)?);
                lock()?.screen(mode as i32)?;
            }
            Stmt::Cls => {
                {
                    let mut g = lock()?;
                    if g.active {
                        g.cls();
                    }
                }
                self.cursor_col = 0;
                self.cursor_row = 1;
                let _ = self.io.console_tx.send(ConsoleMsg::Clear);
            }
            Stmt::Color(fg, bg) => {
                let fv = match fg {
                    Some(e) => Some(to_int(num(self, e)?)),
                    None => None,
                };
                let bv = match bg {
                    Some(e) => Some(to_int(num(self, e)?)),
                    None => None,
                };
                let mut g = lock()?;
                if g.active {
                    if let Some(c) = fv {
                        g.fg_color = qb_color(c.clamp(0, 255) as u32);
                    }
                    if let Some(c) = bv {
                        g.bg_color = qb_color(c.clamp(0, 255) as u32);
                    }
                } else {
                    let _ = self.io.console_tx.send(ConsoleMsg::Color {
                        fg: fv.map(|c| (c.rem_euclid(32)) as u8),
                        bg: bv.map(|c| (c.rem_euclid(16)) as u8),
                    });
                }
            }
            Stmt::Pset(x, y, c) => {
                let (xv, yv, cv) = (coord(self, x)?, coord(self, y)?, color(self, c)?);
                let mut g = lock()?;
                need_graphics(&g)?;
                g.pset(xv, yv, cv);
            }
            Stmt::Line(p1, x2, y2, c, is_box, is_filled) => {
                let start = match p1 {
                    Some((a, b)) => Some((coord(self, a)?, coord(self, b)?)),
                    None => None,
                };
                let (x2v, y2v, cv) = (coord(self, x2)?, coord(self, y2)?, color(self, c)?);
                let mut g = lock()?;
                need_graphics(&g)?;
                let (x1v, y1v) = start.unwrap_or(g.last_point);
                if *is_box {
                    g.rect(x1v, y1v, x2v, y2v, cv, *is_filled);
                } else {
                    g.line(x1v, y1v, x2v, y2v, cv);
                }
            }
            Stmt::Circle(x, y, r, c) => {
                let (xv, yv, rv, cv) = (
                    coord(self, x)?,
                    coord(self, y)?,
                    coord(self, r)?,
                    color(self, c)?,
                );
                let mut g = lock()?;
                need_graphics(&g)?;
                g.circle(xv, yv, rv, cv);
            }
            Stmt::Paint(x, y, fill, border) => {
                let (xv, yv) = (coord(self, x)?, coord(self, y)?);
                let (fv, bv) = (color(self, fill)?, color(self, border)?);
                let mut g = lock()?;
                need_graphics(&g)?;
                let f = fv.unwrap_or(g.fg_color);
                g.paint(xv, yv, f, bv.unwrap_or(f));
            }
            Stmt::Locate(row, col) => {
                let r = match row {
                    Some(e) => Some(to_int(num(self, e)?)),
                    None => None,
                };
                let c = match col {
                    Some(e) => Some(to_int(num(self, e)?)),
                    None => None,
                };
                if r.is_some_and(|r| !(1..=255).contains(&r))
                    || c.is_some_and(|c| !(1..=255).contains(&c))
                {
                    return Err(
                        "Illegal function call: LOCATE row and column must be 1-255".to_string()
                    );
                }
                if let Some(r) = r {
                    self.cursor_row = r as usize;
                }
                if let Some(c) = c {
                    self.cursor_col = c as usize - 1;
                }
                let _ = self.io.console_tx.send(ConsoleMsg::Locate {
                    row: r.map(|r| r as u16),
                    col: c.map(|c| c as u16),
                });
            }
            _ => {}
        }
        Ok(())
    }

    // ---- Input --------------------------------------------------------------

    fn read_console_line(&mut self, prompt: String) -> Result<String, String> {
        let _ = self.io.console_tx.send(ConsoleMsg::InputPrompt(prompt));
        loop {
            match self.io.input_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    self.cursor_col = 0;
                    self.cursor_row += 1;
                    return Ok(line);
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.io.cancel.load(Ordering::Relaxed) {
                        return Err(INTERRUPTED.to_string());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Input past end of file (no more keyboard input)".to_string())
                }
            }
        }
    }

    fn do_input(
        &mut self,
        prompt: Option<&str>,
        question: bool,
        vars: &[LValue],
    ) -> Result<(), String> {
        let text = format!(
            "{}{}",
            prompt.unwrap_or(""),
            if question { "? " } else { "" }
        );
        let mut paths = Vec::with_capacity(vars.len());
        for lv in vars {
            let (root, segs) = self.lvalue_path(lv)?;
            let t = self.path_type(&root, &segs);
            paths.push((root, segs, t));
        }
        'redo: loop {
            let line = self.read_console_line(text.clone())?;
            let fields = if paths.len() == 1 && paths[0].2.is_string() {
                vec![line.trim().trim_matches('"').to_string()]
            } else {
                split_input_fields(&line)
            };
            if fields.len() != paths.len() {
                self.console_print("Redo from start".to_string(), true);
                continue;
            }
            let mut values = Vec::with_capacity(fields.len());
            for (field, (_, _, t)) in fields.into_iter().zip(&paths) {
                if t.is_string() {
                    values.push(Value::Str(field));
                } else {
                    match parse_number_field(&field) {
                        Some(n) => values.push(Value::Number(n)),
                        None => {
                            self.console_print("Redo from start".to_string(), true);
                            continue 'redo;
                        }
                    }
                }
            }
            for ((root, segs, _), v) in paths.iter().zip(values) {
                self.assign_path(root, segs, v)?;
            }
            return Ok(());
        }
    }

    fn do_file_input(&mut self, handle: &Expr, vars: &[LValue]) -> Result<(), String> {
        let h = self.file_number(handle)?;
        for lv in vars {
            let field = match self.files.get_mut(&h) {
                Some(FileHandle::Input(r)) => read_file_field(r)?,
                Some(_) => return Err(format!("Bad file mode: file #{} is not open for INPUT", h)),
                None => return Err(format!("Bad file name or number: file #{} is not open", h)),
            }
            .ok_or_else(|| format!("Input past end of file #{}", h))?;
            let (root, segs) = self.lvalue_path(lv)?;
            let v = if self.path_type(&root, &segs).is_string() {
                Value::Str(field)
            } else {
                Value::Number(parse_number_field(&field).ok_or_else(|| {
                    format!("Type mismatch: '{}' in file #{} is not a number", field, h)
                })?)
            };
            self.assign_path(&root, &segs, v)?;
        }
        Ok(())
    }

    fn do_line_input(
        &mut self,
        handle: Option<&Expr>,
        prompt: Option<&str>,
        var: &LValue,
    ) -> Result<(), String> {
        let line = match handle {
            Some(e) => {
                let h = self.file_number(e)?;
                let r = match self.files.get_mut(&h) {
                    Some(FileHandle::Input(r)) => r,
                    Some(_) => {
                        return Err(format!("Bad file mode: file #{} is not open for INPUT", h))
                    }
                    None => {
                        return Err(format!("Bad file name or number: file #{} is not open", h))
                    }
                };
                let mut buf = Vec::new();
                let n = r.read_until(b'\n', &mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err(format!("Input past end of file #{}", h));
                }
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                String::from_utf8_lossy(&buf).into_owned()
            }
            None => self.read_console_line(prompt.unwrap_or("").to_string())?,
        };
        let (root, segs) = self.lvalue_path(var)?;
        self.assign_path(&root, &segs, Value::Str(line))
    }

    fn do_read(&mut self, vars: &[LValue]) -> Result<(), String> {
        for lv in vars {
            let item = self
                .data
                .get(self.data_ptr)
                .cloned()
                .ok_or_else(|| "Out of DATA: READ past the last DATA item".to_string())?;
            self.data_ptr += 1;
            let (root, segs) = self.lvalue_path(lv)?;
            let v = if self.path_type(&root, &segs).is_string() {
                match item {
                    DataItem::Quoted(s) | DataItem::Unquoted(s) => Value::Str(s),
                }
            } else {
                match item {
                    DataItem::Unquoted(s) => {
                        Value::Number(parse_number_field(&s).ok_or_else(|| {
                            format!("Syntax error in DATA: '{}' is not a number", s)
                        })?)
                    }
                    DataItem::Quoted(s) => {
                        return Err(format!(
                            "Syntax error in DATA: \"{}\" cannot be read into a numeric variable",
                            s
                        ))
                    }
                }
            };
            self.assign_path(&root, &segs, v)?;
        }
        Ok(())
    }

    fn do_mid_assign(
        &mut self,
        target: &LValue,
        start: &Expr,
        len: Option<&Expr>,
        value: &Expr,
    ) -> Result<(), String> {
        let (root, segs) = self.lvalue_path(target)?;
        let current: Vec<char> = self.read_path(&root, &segs)?.as_string()?.chars().collect();
        let start = to_int(self.eval(start)?.as_number()?);
        if start < 1 || start as usize > current.len() {
            return Err("Illegal function call: MID$ start is outside the string".to_string());
        }
        let replacement: Vec<char> = self.eval(value)?.as_string()?.chars().collect();
        let mut n = replacement.len();
        if let Some(l) = len {
            n = n.min(to_int(self.eval(l)?.as_number()?).max(0) as usize);
        }
        let from = start as usize - 1;
        n = n.min(current.len() - from);
        let mut out = current;
        out[from..from + n].copy_from_slice(&replacement[..n]);
        self.assign_path(&root, &segs, Value::Str(out.into_iter().collect()))
    }

    fn do_sleep(&mut self, arg: Option<&Expr>) -> Result<(), String> {
        let secs = match arg {
            Some(e) => self.eval(e)?.as_number()?,
            None => 0.0,
        };
        let graphics_active = self.io.graphics.lock().map(|g| g.active).unwrap_or(false);
        if secs <= 0.0 && !self.io.interactive && !graphics_active {
            return Ok(());
        }
        let deadline =
            (secs > 0.0).then(|| Instant::now() + Duration::from_secs_f64(secs.min(1e6)));
        loop {
            if self.io.cancel.load(Ordering::Relaxed) {
                return Err(INTERRUPTED.to_string());
            }
            if self.io.keys.lock().map(|k| !k.is_empty()).unwrap_or(false) {
                return Ok(());
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // ---- Files --------------------------------------------------------------

    fn type_size(&self, vt: &VarType) -> usize {
        match vt {
            VarType::Integer => 2,
            VarType::Long | VarType::Single | VarType::Auto => 4,
            VarType::Double => 8,
            VarType::Str => 0,
            VarType::FixedStr(n) => *n,
            VarType::UserType(tn) => self
                .types
                .get(tn)
                .map(|fs| fs.iter().map(|(_, t)| self.type_size(t)).sum())
                .unwrap_or(0),
        }
    }

    fn serialize(&self, v: &Value, vt: &VarType, out: &mut Vec<u8>) -> Result<(), String> {
        match (vt, v) {
            (VarType::Integer, Value::Number(n)) => out.extend((*n as i16).to_le_bytes()),
            (VarType::Long, Value::Number(n)) => out.extend((*n as i32).to_le_bytes()),
            (VarType::Single | VarType::Auto, Value::Number(n)) => {
                out.extend((*n as f32).to_le_bytes())
            }
            (VarType::Double, Value::Number(n)) => out.extend(n.to_le_bytes()),
            (VarType::FixedStr(len), Value::Str(s)) => {
                let mut bytes = s.as_bytes().to_vec();
                bytes.resize(*len, b' ');
                out.extend(bytes);
            }
            (VarType::Str, Value::Str(s)) => out.extend(s.as_bytes()),
            (VarType::UserType(tn), Value::Record(r)) => {
                let fields = self
                    .types
                    .get(tn)
                    .ok_or_else(|| format!("Unknown type '{}'", tn))?;
                for ((_, ft), (_, fv)) in fields.iter().zip(&r.fields) {
                    self.serialize(fv, ft, out)?;
                }
            }
            _ => return Err("Type mismatch in PUT".to_string()),
        }
        Ok(())
    }

    fn deserialize(&self, vt: &VarType, buf: &[u8]) -> Result<Value, String> {
        let take = |n: usize| -> Vec<u8> {
            let mut b = buf[..n.min(buf.len())].to_vec();
            b.resize(n, 0);
            b
        };
        Ok(match vt {
            VarType::Integer => {
                Value::Number(i16::from_le_bytes(take(2).try_into().unwrap()) as f64)
            }
            VarType::Long => Value::Number(i32::from_le_bytes(take(4).try_into().unwrap()) as f64),
            VarType::Single | VarType::Auto => {
                Value::Number(f32::from_le_bytes(take(4).try_into().unwrap()) as f64)
            }
            VarType::Double => Value::Number(f64::from_le_bytes(take(8).try_into().unwrap())),
            VarType::FixedStr(n) => {
                Value::Str(String::from_utf8_lossy(&take(*n)).replace('\0', " "))
            }
            VarType::Str => Value::Str(
                String::from_utf8_lossy(buf)
                    .trim_end_matches('\0')
                    .to_string(),
            ),
            VarType::UserType(tn) => {
                let fields = self
                    .types
                    .get(tn)
                    .ok_or_else(|| format!("Unknown type '{}'", tn))?;
                let mut offset = 0;
                let mut out = Vec::with_capacity(fields.len());
                for (fname, ft) in fields {
                    let size = self.type_size(ft);
                    let end = (offset + size).min(buf.len());
                    let slice = if offset < buf.len() {
                        &buf[offset..end]
                    } else {
                        &[]
                    };
                    out.push((fname.clone(), self.deserialize(ft, slice)?));
                    offset += size;
                }
                Value::Record(RecordValue {
                    type_name: tn.clone(),
                    fields: out,
                })
            }
        })
    }

    fn exec_file_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Open {
                filename,
                mode,
                handle,
                record_len,
            } => {
                let fname = self.eval(filename)?.as_string()?;
                let h = self.file_number(handle)?;
                if self.files.contains_key(&h) {
                    return Err(format!("File already open: #{}", h));
                }
                let rlen = match record_len {
                    Some(e) => {
                        let n = to_int(self.eval(e)?.as_number()?);
                        if !(1..=32767).contains(&n) {
                            return Err(format!("Bad record length: {}", n));
                        }
                        n as usize
                    }
                    None => 128,
                };
                let open_err = |e: std::io::Error| match e.kind() {
                    std::io::ErrorKind::NotFound => format!("File not found: {}", fname),
                    _ => format!("Path/File access error: {}: {}", fname, e),
                };
                let entry = match mode {
                    FileMode::Input => {
                        FileHandle::Input(BufReader::new(File::open(&fname).map_err(open_err)?))
                    }
                    FileMode::Output => FileHandle::Output {
                        writer: BufWriter::new(File::create(&fname).map_err(open_err)?),
                        col: 0,
                    },
                    FileMode::Append => FileHandle::Output {
                        writer: BufWriter::new(
                            OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&fname)
                                .map_err(open_err)?,
                        ),
                        col: 0,
                    },
                    FileMode::Random => FileHandle::Random {
                        file: OpenOptions::new()
                            .read(true)
                            .write(true)
                            .create(true)
                            .truncate(false)
                            .open(&fname)
                            .map_err(open_err)?,
                        record_len: rlen,
                    },
                };
                self.files.insert(h, entry);
            }
            Stmt::Close(handles) => {
                if handles.is_empty() {
                    self.files.clear();
                }
                for e in handles {
                    let h = self.file_number(e)?;
                    if let Some(FileHandle::Output { mut writer, .. }) = self.files.remove(&h) {
                        writer.flush().map_err(|e| e.to_string())?;
                    }
                }
            }
            Stmt::Kill(e) => {
                let name = self.eval(e)?.as_string()?;
                std::fs::remove_file(&name).map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => format!("File not found: {}", name),
                    _ => format!("Path/File access error: {}: {}", name, e),
                })?;
            }
            Stmt::Get {
                handle,
                record,
                var,
            }
            | Stmt::Put {
                handle,
                record,
                var,
            } => {
                let is_get = matches!(stmt, Stmt::Get { .. });
                let h = self.file_number(handle)?;
                let rec = match record {
                    Some(e) => {
                        let n = to_int(self.eval(e)?.as_number()?);
                        if n < 1 {
                            return Err(format!("Bad record number: {}", n));
                        }
                        Some(n as u64)
                    }
                    None => None,
                };
                let (root, segs) = self.lvalue_path(var)?;
                let vt = self.path_type(&root, &segs);
                let bytes = if is_get {
                    None
                } else {
                    let v = self.read_path(&root, &segs)?;
                    let mut out = Vec::new();
                    self.serialize(&v, &vt, &mut out)?;
                    Some(out)
                };
                let (file, rlen) = match self.files.get_mut(&h) {
                    Some(FileHandle::Random { file, record_len }) => (file, *record_len),
                    Some(_) => {
                        return Err(format!("Bad file mode: file #{} is not open for RANDOM", h))
                    }
                    None => {
                        return Err(format!("Bad file name or number: file #{} is not open", h))
                    }
                };
                if let Some(rn) = rec {
                    file.seek(SeekFrom::Start((rn - 1) * rlen as u64))
                        .map_err(|e| e.to_string())?;
                }
                match bytes {
                    Some(mut out) => {
                        if out.len() > rlen {
                            return Err(format!(
                                "Bad record length: {} bytes do not fit in LEN = {}",
                                out.len(),
                                rlen
                            ));
                        }
                        out.resize(rlen, 0);
                        file.write_all(&out)
                            .map_err(|e| format!("Device I/O error: {}", e))?;
                    }
                    None => {
                        let mut buf = vec![0u8; rlen];
                        let mut read = 0;
                        while read < rlen {
                            let n = file.read(&mut buf[read..]).map_err(|e| e.to_string())?;
                            if n == 0 {
                                break;
                            }
                            read += n;
                        }
                        let size = match vt {
                            VarType::Str => rlen,
                            ref t => self.type_size(t),
                        };
                        let v = self.deserialize(&vt, &buf[..size.min(rlen)])?;
                        self.assign_path(&root, &segs, v)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn collect_consts(stmts: &[Stmt], out: &mut Vec<(usize, String, Expr)>) {
    let mut line = 1;
    for s in stmts {
        match s {
            Stmt::SrcLine(l) => line = *l,
            Stmt::Const(n, e) => out.push((line, n.clone(), e.clone())),
            Stmt::SubDef { body, .. } | Stmt::FuncDef { body, .. } => collect_consts(body, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    pub(crate) fn run_source(src: &str, inputs: &[&str]) -> Result<Vec<String>, String> {
        let tokens = crate::lexer::tokenize(src)?;
        let program = crate::parser::parse(tokens)?;
        let (console_tx, console_rx) = mpsc::channel();
        let (input_tx, input_rx) = mpsc::channel();
        for input in inputs {
            input_tx.send((*input).to_string()).unwrap();
        }
        drop(input_tx);
        let io = Io {
            console_tx,
            input_rx,
            cancel: Arc::new(AtomicBool::new(false)),
            graphics: Arc::new(Mutex::new(SharedGraphics::new())),
            keys: Arc::new(Mutex::new(VecDeque::new())),
            interactive: false,
        };
        let result = spawn(program, io).join().expect("interpreter panicked");

        let mut lines = vec![String::new()];
        for msg in console_rx.try_iter() {
            match msg {
                ConsoleMsg::Print { text, newline } => {
                    for (i, part) in text.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(String::new());
                        }
                        lines.last_mut().unwrap().push_str(part);
                    }
                    if newline {
                        lines.push(String::new());
                    }
                }
                ConsoleMsg::Clear => lines = vec![String::new()],
                _ => {}
            }
        }
        result.map(|_| lines.into_iter().filter(|line| !line.is_empty()).collect())
    }

    fn out(src: &str) -> Vec<String> {
        run_source(src, &[]).unwrap()
    }

    #[test]
    fn assigns_dim_string_without_suffix() {
        assert_eq!(out("DIM S AS STRING\nS = \"OK\"\nPRINT S\n"), vec!["OK"]);
    }

    #[test]
    fn records_and_arrays_of_records() {
        assert_eq!(
            out("TYPE PLAYER\nNAME AS STRING\nSCORE AS INTEGER\nEND TYPE\nDIM P AS PLAYER\nP.NAME = \"Ada\"\nP.SCORE = 42\nPRINT P.NAME; \" scored \"; P.SCORE\n"),
            vec!["Ada scored 42"]
        );
        assert_eq!(
            out("TYPE P\nN AS STRING\nEND TYPE\nDIM A(3) AS P\nA(1).N = \"x\"\nLET A(2).N = \"y\"\nPRINT A(1).N; A(2).N\n"),
            vec!["xy"]
        );
    }

    #[test]
    fn duplicate_line_numbers_are_errors() {
        let err = run_source("10 PRINT \"FIRST\"\n10 PRINT \"SECOND\"\n", &[]).unwrap_err();
        assert!(err.contains("Duplicate label: 10"), "{}", err);
    }

    #[test]
    fn array_bounds_option_base_and_to() {
        assert_eq!(
            out("DIM A(1)\nA(0) = 7\nA(1) = 8\nPRINT A(0); \",\"; A(1)\n"),
            vec!["7,8"]
        );
        assert_eq!(
            out("OPTION BASE 1\nDIM A(2)\nA(1) = 10\nA(2) = 20\nPRINT A(1); \",\"; A(2)\n"),
            vec!["10,20"]
        );
        let err = run_source("OPTION BASE 1\nDIM A(2)\nA(0) = 99\n", &[]).unwrap_err();
        assert!(err.contains("Subscript out of range"), "{}", err);
        let err = run_source("OPTION BASE 2\nDIM A(2)\n", &[]).unwrap_err();
        assert!(err.contains("OPTION BASE must be 0 or 1"), "{}", err);
        assert_eq!(
            out("DIM A(-2 TO 2, 1 TO 3)\nA(-2, 3) = 5\nPRINT LBOUND(A); UBOUND(A); UBOUND(A, 2); A(-2, 3)\n"),
            vec!["-2235"]
        );
        // Undeclared arrays get QBasic's implicit 0-10 bounds.
        assert_eq!(out("B(10) = 4\nPRINT B(10); UBOUND(B)\n"), vec!["410"]);
    }

    #[test]
    fn input_reads_typed_values() {
        assert_eq!(
            run_source("DIM S AS STRING\nINPUT S\nPRINT S\n", &["hello"]).unwrap(),
            vec!["hello"]
        );
        assert_eq!(
            run_source("DIM A(1)\nINPUT A(0)\nPRINT A(0)\n", &["7"]).unwrap(),
            vec!["7"]
        );
        assert_eq!(
            run_source("INPUT \"n\", N\nPRINT N * 2\n", &["abc", "21"]).unwrap(),
            vec!["Redo from start", "42"]
        );
        let err = run_source("INPUT N\n", &[]).unwrap_err();
        assert!(err.contains("Input past end of file"), "{}", err);
    }

    #[test]
    fn forward_and_declared_functions() {
        assert_eq!(
            out("S$ = MAKE()\nPRINT S$\nFUNCTION MAKE() AS STRING\nMAKE = \"OK\"\nEND FUNCTION\n"),
            vec!["OK"]
        );
        let tokens = crate::lexer::tokenize(
            "DECLARE FUNCTION MAKE() AS STRING\nS$ = MAKE()\nFUNCTION MAKE$\nEND FUNCTION\n",
        )
        .unwrap();
        assert!(crate::parser::parse(tokens).is_ok());
    }

    #[test]
    fn redim_preserve() {
        assert_eq!(
            out("DIM A(1)\nA(0) = 42\nA(1) = 99\nREDIM PRESERVE A(2)\nPRINT A(0); \",\"; A(1); \",\"; A(2)\n"),
            vec!["42,99,0"]
        );
        assert_eq!(
            out("DIM A(1) AS STRING\nA(0) = \"Ada\"\nREDIM PRESERVE A(2)\nPRINT A(0); \":\"; A(2)\n"),
            vec!["Ada:"]
        );
    }

    #[test]
    fn array_parameters_are_passed_by_reference() {
        assert_eq!(
            out("DIM A(1)\nCALL SETFIRST(A())\nPRINT A(0)\nSUB SETFIRST(A() AS DOUBLE)\nA(0) = 12\nEND SUB\n"),
            vec!["12"]
        );
    }

    #[test]
    fn builtin_errors_do_not_panic() {
        let err = run_source("PRINT SIN()\n", &[]).unwrap_err();
        assert!(err.contains("SIN: missing argument 1"), "{}", err);
        for src in [
            "PRINT 5 \\ 0.4\n",
            "PRINT 5 MOD 0.4\n",
            "PRINT -9223372036854775808 \\ -1\n",
            "PRINT SPACE$(1E18)\n",
            "PRINT CHR$(-1)\n",
            "DIM A(100000, 100000)\n",
        ] {
            assert!(run_source(src, &[]).is_err(), "{}", src);
        }
    }

    #[test]
    fn constants_are_read_only() {
        let err = run_source("CONST X = 1\nX = 2\n", &[]).unwrap_err();
        assert!(err.contains("Cannot assign to constant"), "{}", err);
        let err = run_source("CONST X = 1\nFOR X = 1 TO 3\nNEXT X\n", &[]).unwrap_err();
        assert!(err.contains("Cannot assign to constant"), "{}", err);
    }

    #[test]
    fn data_read_restore() {
        assert_eq!(
            out("DATA 10, \"ADA\"\nREAD N, NAME$\nPRINT N; \":\"; NAME$\nRESTORE\nREAD AGAIN\nPRINT AGAIN\n"),
            vec!["10:ADA", "10"]
        );
        assert_eq!(
            out("10 DATA 1\n20 DATA 2\nREAD A\nRESTORE 20\nREAD B\nPRINT A; \",\"; B\n"),
            vec!["1,2"]
        );
        let err = run_source("DATA 1\nREAD A, B\n", &[]).unwrap_err();
        assert!(err.contains("Out of DATA"), "{}", err);
        assert_eq!(
            out("DATA New York, red\nREAD A$, B$\nPRINT A$; \"|\"; B$\n"),
            vec!["New York|red"]
        );
        let err = run_source("DATA abc\nREAD N\n", &[]).unwrap_err();
        assert!(err.contains("Syntax error in DATA"), "{}", err);
    }

    #[test]
    fn common_classic_basic_syntax_runs() {
        assert_eq!(
            run_source(
                "OPTION BASE 1\nDEFINT A-Z\nDECLARE SUB IGNORED()\nREDIM A(2)\nA(1) = 10\nA(2) = 20\nSWAP A(1), A(2)\nPRINT A(1); \",\"; A(2)\nN = 2\nON N GOTO 100, 200\n100 PRINT \"WRONG\": END\n200 LINE INPUT \"Name\"; NAME$\nPRINT NAME$\nSUB IGNORED\nEND SUB\n",
                &["Ada, Lovelace"],
            )
            .unwrap(),
            vec!["20,10", "Ada, Lovelace"]
        );
    }

    #[test]
    fn defstr_sets_default_string_variables() {
        assert_eq!(out("DEFSTR A-Z\nA = \"OK\"\nPRINT A\n"), vec!["OK"]);
    }

    #[test]
    fn gosub_inside_loops_returns_to_the_loop() {
        assert_eq!(
            out("FOR I = 1 TO 3\nGOSUB show\nNEXT I\nPRINT \"after\"\nEND\nshow:\nPRINT \"I=\"; I\nRETURN\n"),
            vec!["I=1", "I=2", "I=3", "after"]
        );
        assert_eq!(
            out("N = 2\nDO WHILE N > 0\nON N GOSUB 100, 200\nN = N - 1\nLOOP\nEND\n100 PRINT \"one\": RETURN\n200 PRINT \"two\": RETURN\n"),
            vec!["two", "one"]
        );
        let err = run_source("RETURN\n", &[]).unwrap_err();
        assert!(err.contains("RETURN without GOSUB"), "{}", err);
    }

    #[test]
    fn shared_and_static_variables() {
        assert_eq!(
            out("DIM SHARED COUNT\nSUB BUMP\nCOUNT = COUNT + 1\nEND SUB\nBUMP\nBUMP\nPRINT \"count=\"; COUNT\n"),
            vec!["count=2"]
        );
        assert_eq!(
            out("X = 5\nSUB S\nSHARED X\nX = X * 2\nEND SUB\nS\nPRINT X\n"),
            vec!["10"]
        );
        // Without SHARED, SUB variables are local.
        assert_eq!(
            out("X = 5\nSUB S\nX = 99\nEND SUB\nS\nPRINT X\n"),
            vec!["5"]
        );
        assert_eq!(
            out("SUB C\nSTATIC N\nN = N + 1\nPRINT N\nEND SUB\nC\nC\nC\n"),
            vec!["1", "2", "3"]
        );
        assert_eq!(
            out("SUB C STATIC\nN = N + 1\nPRINT N\nEND SUB\nC\nC\n"),
            vec!["1", "2"]
        );
    }

    #[test]
    fn end_inside_sub_stops_the_program() {
        assert_eq!(
            out("SUB QUIT\nPRINT \"ending\"\nEND\nEND SUB\nQUIT\nPRINT \"still running\"\n"),
            vec!["ending"]
        );
    }

    #[test]
    fn zero_argument_builtins_work_without_parentheses() {
        let lines = out("RANDOMIZE TIMER\nFOR I = 1 TO 20\nPRINT INT(RND * 6) + 1\nNEXT I\n");
        let distinct: HashSet<_> = lines.iter().collect();
        assert!(distinct.len() > 1, "{:?}", lines);
        assert!(lines.iter().all(|l| ("1"..="6").contains(&l.as_str())));
    }

    #[test]
    fn functions_are_evaluated_once_and_nest() {
        assert_eq!(
            out("DIM A(5)\nFUNCTION F(BYVAL N)\nPRINT \"F called\"\nF = N\nEND FUNCTION\nA(1) = 9\nPRINT A(F(1))\n"),
            vec!["F called", "9"]
        );
        assert_eq!(
            out("FUNCTION DBL(N)\nDBL = N * 2\nEND FUNCTION\nPRINT DBL(ABS(-3)); DBL(DBL(DBL(1)))\n"),
            vec!["68"]
        );
    }

    #[test]
    fn byref_arguments_write_back_once() {
        assert_eq!(
            out("SUB INC(X)\nX = X + 1\nEND SUB\nA = 1\nINC A\nDIM B(2)\nI = 1\nINC B(I)\nPRINT A; B(1)\n"),
            vec!["21"]
        );
    }

    #[test]
    fn instr_with_start_position() {
        assert_eq!(
            out("PRINT INSTR(3, \"hello hello\", \"h\"); INSTR(\"abc\", \"c\")\n"),
            vec!["73"]
        );
    }

    #[test]
    fn on_error_resume_next_and_retry() {
        assert_eq!(
            out("ON ERROR GOTO h\nX = 1 / 0\nPRINT \"after\"\nEND\nh:\nPRINT \"handled\"; ERR\nRESUME NEXT\n"),
            vec!["handled11", "after"]
        );
        assert_eq!(
            out("ON ERROR GOTO fix\nD = 0\nPRINT 10 / D\nEND\nfix:\nD = 2\nRESUME\n"),
            vec!["5"]
        );
        assert_eq!(
            out(
                "ON ERROR GOTO h\nERROR 53\nEND\nh:\nPRINT ERR\nRESUME done\ndone:\nPRINT \"ok\"\n"
            ),
            vec!["53", "ok"]
        );
        let err = run_source(
            "ON ERROR GOTO h\nON ERROR GOTO 0\nX = 1 / 0\nh:\nRESUME NEXT\n",
            &[],
        )
        .unwrap_err();
        assert!(err.contains("Division by zero"), "{}", err);
    }

    #[test]
    fn integer_variables_round_and_check_overflow() {
        assert_eq!(
            out("DIM X AS INTEGER\nX = 3.7\nA% = 7 / 2\nB% = 5 / 2\nPRINT X; \" \"; A%; \" \"; B%\n"),
            vec!["4 4 2"]
        );
        let err = run_source("A% = 40000\n", &[]).unwrap_err();
        assert!(err.contains("Overflow"), "{}", err);
        assert_eq!(
            out("PRINT 0.1 + 0.2; \" \"; 2 ^ 3 ^ 2; \" \"; -2 ^ 2; \" \"; 1E5\n"),
            vec!["0.3 64 -4 100000"]
        );
    }

    #[test]
    fn deep_recursion_reports_out_of_stack_space() {
        let src = "FUNCTION D(BYVAL N)\nIF N = 0 THEN\nD = 0\nELSE\nD = D(N - 1) + 1\nEND IF\nEND FUNCTION\nPRINT D(500)\nPRINT D(100000)\n";
        let err = run_source(src, &[]).unwrap_err();
        assert!(err.contains("Out of stack space"), "{}", err);
        assert_eq!(out("FUNCTION D(BYVAL N)\nIF N = 0 THEN D = 0 ELSE D = D(N - 1) + 1\nEND FUNCTION\nPRINT D(900)\n"), vec!["900"]);
    }

    #[test]
    fn runtime_errors_report_line_numbers() {
        let err = run_source("PRINT \"ok\"\nPRINT \"ok2\"\nX = 1 / 0\n", &[]).unwrap_err();
        assert_eq!(err, "Line 3: Division by zero");
        let err = run_source("10 PRINT 1\n20 X = 1 / 0\n", &[]).unwrap_err();
        assert_eq!(err, "Line 2 (BASIC line 20): Division by zero");
        let err = run_source("SUB S\nPRINT 1\nY = LOG(0)\nEND SUB\nS\n", &[]).unwrap_err();
        assert!(err.starts_with("Line 3: Illegal function call"), "{}", err);
    }

    #[test]
    fn print_zones_tab_and_using() {
        assert_eq!(out("PRINT \"a\", \"b\"\n"), vec!["a             b"]);
        assert_eq!(out("PRINT \"ab\"; TAB(6); \"c\"\n"), vec!["ab   c"]);
        assert_eq!(out("PRINT USING \"##.##\"; 3.14159\n"), vec![" 3.14"]);
        assert_eq!(out("WRITE \"a\", 1, -2.5\n"), vec!["\"a\",1,-2.5"]);
    }

    #[test]
    fn mid_statement_def_fn_and_misc() {
        assert_eq!(
            out("A$ = \"hello\"\nMID$(A$, 1, 1) = \"J\"\nPRINT A$\n"),
            vec!["Jello"]
        );
        assert_eq!(out("DEF FNA(X) = X * 2\nPRINT FNA(21)\n"), vec!["42"]);
        assert_eq!(
            out("PRINT &HFF; HEX$(-1); VAL(\"1.5E2xyz\")\n"),
            vec!["255FFFF150"]
        );
        assert_eq!(out("? \"q\"\nPRINT A: PRINT A\n"), vec!["q", "0", "0"]);
        assert_eq!(
            out("FOR I = 1 TO 3: PRINT I;: NEXT I\nPRINT\n"),
            vec!["123"]
        );
        assert_eq!(
            out("FOR I = 1 TO 2\nFOR J = 1 TO 2\nPRINT I; J\nNEXT J, I\n"),
            vec!["11", "12", "21", "22"]
        );
        assert_eq!(
            out("PRINT CHR$(65); ASC(CHR$(201)); LEN(CHR$(219))\n"),
            vec!["A2011"]
        );
    }

    #[test]
    fn files_eof_line_input_and_random_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.txt");
        let p = path.to_string_lossy().replace('\\', "\\\\");
        let src = format!(
            "F = FREEFILE\nOPEN \"{p}\" FOR OUTPUT AS #F\nPRINT #F, \"a, b\"\nWRITE #F, \"x,y\", 3\nCLOSE #F\nOPEN \"{p}\" FOR INPUT AS #1\nLINE INPUT #1, L$\nPRINT L$\nDO WHILE NOT EOF(1)\nINPUT #1, V$\nPRINT \"[\"; V$; \"]\"\nLOOP\nCLOSE\n"
        );
        assert_eq!(out(&src), vec!["a, b", "[x,y]", "[3]"]);

        let rpath = dir
            .path()
            .join("r.dat")
            .to_string_lossy()
            .replace('\\', "\\\\");
        let src = format!(
            "TYPE REC\nID AS INTEGER\nNM AS STRING * 5\nAMT AS DOUBLE\nEND TYPE\nDIM R AS REC\nOPEN \"{rpath}\" FOR RANDOM AS #1 LEN = LEN(R)\nR.ID = 7: R.NM = \"Bob\": R.AMT = 2.5\nPUT #1, 2, R\nDIM S AS REC\nGET #1, 2, S\nPRINT S.ID; \"[\"; S.NM; \"]\"; S.AMT; LEN(R); LOF(1)\nCLOSE #1\n"
        );
        assert_eq!(out(&src), vec!["7[Bob  ]2.51530"]);
    }

    #[test]
    fn exit_do_inside_while_leaves_the_do() {
        assert_eq!(
            out("N = 0\nDO\nWHILE 1\nN = N + 1\nEXIT DO\nWEND\nLOOP\nPRINT N\nWHILE 1: EXIT WHILE: WEND\nPRINT \"ok\"\n"),
            vec!["1", "ok"]
        );
    }

    #[test]
    fn select_case_one_liners() {
        assert_eq!(
            out("X = 1\nSELECT CASE X\nCASE 1: PRINT \"one\"\nCASE ELSE\nPRINT \"other\"\nEND SELECT\n"),
            vec!["one"]
        );
    }
}
