#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    StringLit(String),
    Identifier(String),
    /// Raw items of a `DATA` statement: `(text, was_quoted)`.
    DataItems(Vec<(String, bool)>),
    Print,
    Let,
    Input,
    If,
    Then,
    Else,
    ElseIf,
    EndIf,
    For,
    To,
    Step,
    Next,
    While,
    Wend,
    Do,
    Loop,
    Until,
    Goto,
    Gosub,
    Return,
    End,
    Dim,
    Data,
    Read,
    Restore,
    Redim,
    Preserve,
    Erase,
    As,
    Integer,
    Single,
    Double,
    Long,
    StringKw,
    Sub,
    Function,
    EndSub,
    EndFunction,
    Call,
    Declare,
    Exit,
    ByRef,
    ByVal,
    Shared,
    Static,
    Const,
    Type,
    EndType,
    Option,
    Base,
    DefInt,
    DefLng,
    DefSng,
    DefDbl,
    DefStr,
    Select,
    Case,
    EndSelect,
    Is,
    Open,
    Close,
    Output,
    Append,
    Random,
    Hash,
    Get,
    Put,
    LenKw,
    Screen,
    Line,
    Circle,
    Pset,
    Color,
    Cls,
    Window,
    Paint,
    Locate,
    Beep,
    Swap,
    Clear,
    Stop,
    System,
    On,
    Error,
    Resume,
    Using,
    Write,
    Kill,
    Mod,
    And,
    Or,
    Not,
    Xor,
    Randomize,
    Sleep,
    Equals,
    Plus,
    Minus,
    Star,
    Slash,
    Backslash,
    Caret,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Colon,
    Dot,
    NotEqual,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    LineNumber(u32),
    Label(String),
    Newline,
    Eof,
}

/// Tokenize BASIC source.
///
/// Every physical source line produces exactly one `Token::Newline`, so the
/// parser can recover 1-based line numbers by counting them.
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = Vec::new();
    for (line_idx, raw_line) in input.lines().enumerate() {
        chars.clear();
        chars.extend(raw_line.trim_end().chars());
        tokenize_line(&chars, &mut tokens).map_err(|e| format!("Line {}: {}", line_idx + 1, e))?;
        tokens.push(Token::Newline);
    }
    tokens.push(Token::Eof);
    Ok(tokens)
}

fn tokenize_line(chars: &[char], tokens: &mut Vec<Token>) -> Result<(), String> {
    let mut i = 0;
    // Number of tokens emitted on this line, used to recognise line numbers
    // and labels, which are only valid at the start of a line.
    let line_start = tokens.len();
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' {
            i += 1;
            continue;
        }
        let at_line_start = tokens.len() == line_start;
        let only_line_number =
            tokens.len() == line_start + 1 && matches!(tokens[line_start], Token::LineNumber(_));
        if at_line_start && c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let n: u32 = s
                .parse()
                .map_err(|_| format!("Line number {} is too large", s))?;
            tokens.push(Token::LineNumber(n));
            continue;
        }
        if c == '\'' {
            break;
        }
        if c == '#' {
            tokens.push(Token::Hash);
            i += 1;
            continue;
        }
        if c == '?' {
            tokens.push(Token::Print);
            i += 1;
            continue;
        }
        if c == '&'
            && i + 1 < chars.len()
            && matches!(chars[i + 1], 'h' | 'H' | 'o' | 'O' | 'b' | 'B')
        {
            let radix = match chars[i + 1].to_ascii_uppercase() {
                'H' => 16,
                'O' => 8,
                _ => 2,
            };
            i += 2;
            let start = i;
            while i < chars.len() && chars[i].is_digit(radix) {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            if s.is_empty() {
                return Err("Expected digits after '&' prefix".to_string());
            }
            let n = i64::from_str_radix(&s, radix)
                .map_err(|_| format!("Number '&{}' is too large", s))?;
            // Optional type suffix (&H7FFF%, &HFFFF&).
            if i < chars.len() && matches!(chars[i], '%' | '&') {
                i += 1;
            }
            tokens.push(Token::Number(n as f64));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let (n, next) = lex_number(chars, i)?;
            i = next;
            tokens.push(Token::Number(n));
            continue;
        }
        if c == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i >= chars.len() {
                return Err("Unterminated string (missing closing \")".to_string());
            }
            let s: String = chars[start..i].iter().collect();
            i += 1;
            tokens.push(Token::StringLit(s));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            if i < chars.len() && matches!(chars[i], '$' | '%' | '!' | '#' | '&') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            if upper == "REM" {
                break;
            }
            let is_label = (at_line_start || only_line_number)
                && i < chars.len()
                && chars[i] == ':'
                && !is_keyword(&upper);
            if is_label {
                i += 1;
                tokens.push(Token::Label(upper));
                continue;
            }
            if upper == "DATA" {
                tokens.push(Token::Data);
                let (items, next) = lex_data_items(chars, i);
                tokens.push(Token::DataItems(items));
                i = next;
                continue;
            }
            if upper == "END" {
                let mut j = i;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }
                let follows = |word: &str| {
                    let len = word.len();
                    j + len <= chars.len()
                        && chars[j..j + len]
                            .iter()
                            .zip(word.chars())
                            .all(|(a, b)| a.to_ascii_uppercase() == b)
                        && (j + len >= chars.len()
                            || !(chars[j + len].is_ascii_alphanumeric() || chars[j + len] == '_'))
                };
                let compound = [
                    ("IF", Token::EndIf),
                    ("SUB", Token::EndSub),
                    ("FUNCTION", Token::EndFunction),
                    ("TYPE", Token::EndType),
                    ("SELECT", Token::EndSelect),
                ]
                .into_iter()
                .find(|(w, _)| follows(w));
                match compound {
                    Some((w, tok)) => {
                        i = j + w.len();
                        tokens.push(tok);
                    }
                    None => tokens.push(Token::End),
                }
                continue;
            }
            tokens.push(keyword_token(&upper).unwrap_or(Token::Identifier(upper)));
            continue;
        }
        let (tok, len) = match c {
            '=' => (Token::Equals, 1),
            '+' => (Token::Plus, 1),
            '-' => (Token::Minus, 1),
            '*' => (Token::Star, 1),
            '/' => (Token::Slash, 1),
            '\\' => (Token::Backslash, 1),
            '^' => (Token::Caret, 1),
            '(' => (Token::LParen, 1),
            ')' => (Token::RParen, 1),
            ',' => (Token::Comma, 1),
            ';' => (Token::Semicolon, 1),
            ':' => (Token::Colon, 1),
            '.' => (Token::Dot, 1),
            '<' if chars.get(i + 1) == Some(&'=') => (Token::LessEqual, 2),
            '<' if chars.get(i + 1) == Some(&'>') => (Token::NotEqual, 2),
            '<' => (Token::Less, 1),
            '>' if chars.get(i + 1) == Some(&'=') => (Token::GreaterEqual, 2),
            '>' => (Token::Greater, 1),
            _ => return Err(format!("Unexpected character '{}'", c)),
        };
        tokens.push(tok);
        i += len;
    }
    Ok(())
}

/// Lex a decimal number with optional fraction, exponent (`E` or `D`) and
/// type suffix (`%`, `&`, `!`, `#`). Returns the value and the next index.
fn lex_number(chars: &[char], mut i: usize) -> Result<(f64, usize), String> {
    let start = i;
    let mut saw_dot = false;
    while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && !saw_dot)) {
        if chars[i] == '.' {
            saw_dot = true;
        }
        i += 1;
    }
    let mut text: String = chars[start..i].iter().collect();
    if i < chars.len() && matches!(chars[i], 'e' | 'E' | 'd' | 'D') {
        let mut j = i + 1;
        if j < chars.len() && matches!(chars[j], '+' | '-') {
            j += 1;
        }
        if j < chars.len() && chars[j].is_ascii_digit() {
            text.push('e');
            text.extend(&chars[i + 1..j]);
            while j < chars.len() && chars[j].is_ascii_digit() {
                text.push(chars[j]);
                j += 1;
            }
            i = j;
        }
    }
    if i < chars.len() && matches!(chars[i], '%' | '&' | '!' | '#') {
        i += 1;
    }
    let n: f64 = text.parse().map_err(|_| format!("Bad number '{}'", text))?;
    if !n.is_finite() {
        return Err(format!("Number '{}' is too large", text));
    }
    Ok((n, i))
}

/// Split the remainder of a `DATA` statement into items. Items are separated
/// by commas; quoted items keep their exact text, unquoted items are trimmed
/// but keep their case. The statement ends at an unquoted colon or the end of
/// the line.
fn lex_data_items(chars: &[char], mut i: usize) -> (Vec<(String, bool)>, usize) {
    let mut items = Vec::new();
    loop {
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i < chars.len() && chars[i] == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            items.push((chars[start..i].iter().collect(), true));
            if i < chars.len() {
                i += 1;
            }
            while i < chars.len() && !matches!(chars[i], ',' | ':') {
                i += 1;
            }
        } else {
            let start = i;
            while i < chars.len() && !matches!(chars[i], ',' | ':') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            items.push((text.trim().to_string(), false));
        }
        if i < chars.len() && chars[i] == ',' {
            i += 1;
            continue;
        }
        break;
    }
    if items.len() == 1 && !items[0].1 && items[0].0.is_empty() {
        items.clear();
    }
    (items, i)
}

fn keyword_token(upper: &str) -> Option<Token> {
    Some(match upper {
        "PRINT" => Token::Print,
        "LET" => Token::Let,
        "INPUT" => Token::Input,
        "IF" => Token::If,
        "THEN" => Token::Then,
        "ELSE" => Token::Else,
        "ELSEIF" => Token::ElseIf,
        "ENDIF" => Token::EndIf,
        "FOR" => Token::For,
        "TO" => Token::To,
        "STEP" => Token::Step,
        "NEXT" => Token::Next,
        "WHILE" => Token::While,
        "WEND" => Token::Wend,
        "DO" => Token::Do,
        "LOOP" => Token::Loop,
        "UNTIL" => Token::Until,
        "GOTO" => Token::Goto,
        "GOSUB" => Token::Gosub,
        "RETURN" => Token::Return,
        "DIM" => Token::Dim,
        "READ" => Token::Read,
        "RESTORE" => Token::Restore,
        "REDIM" => Token::Redim,
        "PRESERVE" => Token::Preserve,
        "ERASE" => Token::Erase,
        "AS" => Token::As,
        "INTEGER" => Token::Integer,
        "LONG" => Token::Long,
        "SINGLE" => Token::Single,
        "DOUBLE" => Token::Double,
        "STRING" => Token::StringKw,
        "SUB" => Token::Sub,
        "FUNCTION" => Token::Function,
        "CALL" => Token::Call,
        "DECLARE" => Token::Declare,
        "EXIT" => Token::Exit,
        "BYREF" => Token::ByRef,
        "BYVAL" => Token::ByVal,
        "SHARED" => Token::Shared,
        "STATIC" => Token::Static,
        "CONST" => Token::Const,
        "TYPE" => Token::Type,
        "OPTION" => Token::Option,
        "BASE" => Token::Base,
        "DEFINT" => Token::DefInt,
        "DEFLNG" => Token::DefLng,
        "DEFSNG" => Token::DefSng,
        "DEFDBL" => Token::DefDbl,
        "DEFSTR" => Token::DefStr,
        "SELECT" => Token::Select,
        "CASE" => Token::Case,
        "IS" => Token::Is,
        "OPEN" => Token::Open,
        "CLOSE" => Token::Close,
        "OUTPUT" => Token::Output,
        "APPEND" => Token::Append,
        "RANDOM" => Token::Random,
        "GET" => Token::Get,
        "PUT" => Token::Put,
        "LEN" => Token::LenKw,
        "SCREEN" => Token::Screen,
        "LINE" => Token::Line,
        "CIRCLE" => Token::Circle,
        "PSET" => Token::Pset,
        "COLOR" => Token::Color,
        "CLS" => Token::Cls,
        "WINDOW" => Token::Window,
        "PAINT" => Token::Paint,
        "LOCATE" => Token::Locate,
        "BEEP" => Token::Beep,
        "SWAP" => Token::Swap,
        "CLEAR" => Token::Clear,
        "STOP" => Token::Stop,
        "SYSTEM" => Token::System,
        "ON" => Token::On,
        "ERROR" => Token::Error,
        "RESUME" => Token::Resume,
        "USING" => Token::Using,
        "WRITE" => Token::Write,
        "KILL" => Token::Kill,
        "MOD" => Token::Mod,
        "AND" => Token::And,
        "OR" => Token::Or,
        "NOT" => Token::Not,
        "XOR" => Token::Xor,
        "RANDOMIZE" => Token::Randomize,
        "SLEEP" => Token::Sleep,
        _ => return None,
    })
}

pub fn is_keyword(s: &str) -> bool {
    matches!(s, "END" | "REM" | "DATA") || keyword_token(s).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Token> {
        tokenize(src).unwrap()
    }

    #[test]
    fn colon_after_variable_is_a_separator_not_a_label() {
        assert_eq!(
            toks("PRINT A: PRINT A"),
            vec![
                Token::Print,
                Token::Identifier("A".into()),
                Token::Colon,
                Token::Print,
                Token::Identifier("A".into()),
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn labels_only_at_line_start() {
        assert_eq!(toks("start: X = 1")[0], Token::Label("START".into()));
        assert_eq!(toks("10 again: PRINT")[1], Token::Label("AGAIN".into()));
    }

    #[test]
    fn exponent_hex_and_suffix_literals() {
        assert_eq!(toks("? 1E5")[1], Token::Number(100000.0));
        assert_eq!(toks("? 2.5D-1")[1], Token::Number(0.25));
        assert_eq!(toks("&HFF")[0], Token::Number(255.0));
        assert_eq!(toks("&O17")[0], Token::Number(15.0));
        assert_eq!(toks("&B101")[0], Token::Number(5.0));
        assert_eq!(toks("X = 5#")[2], Token::Number(5.0));
        // E followed by a letter is not an exponent.
        assert_eq!(toks("? 1ELSE")[1..3], [Token::Number(1.0), Token::Else]);
    }

    #[test]
    fn question_mark_is_print() {
        assert_eq!(toks("? 1")[0], Token::Print);
    }

    #[test]
    fn data_items_keep_case_and_spaces() {
        assert_eq!(
            toks("DATA New York, \"a, b\", 5 : PRINT")[..4],
            [
                Token::Data,
                Token::DataItems(vec![
                    ("New York".into(), false),
                    ("a, b".into(), true),
                    ("5".into(), false)
                ]),
                Token::Colon,
                Token::Print
            ]
        );
    }

    #[test]
    fn errors_report_line_numbers() {
        let err = tokenize("PRINT 1\nPRINT \"oops\n").unwrap_err();
        assert!(err.starts_with("Line 2:"), "{}", err);
    }

    #[test]
    fn end_compounds() {
        assert_eq!(toks("END IF")[0], Token::EndIf);
        assert_eq!(toks("end   select")[0], Token::EndSelect);
        assert_eq!(toks("END")[0], Token::End);
        assert_eq!(toks("END IFFY")[0], Token::End);
    }
}
