//! Number formatting for `PRINT`/`STR$` and the `PRINT USING` formatter.

/// Formats a number for display: integers print without a fraction, other
/// values are rounded to 15 significant digits so binary floating-point noise
/// (`0.1 + 0.2`) does not leak into output, and very large or small values use
/// QBasic-style exponent notation (`1E+300`).
pub fn format_number(n: f64) -> String {
    if n == 0.0 {
        return "0".to_string();
    }
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let rounded: f64 = format!("{:.14e}", n).parse().unwrap_or(n);
    let abs = rounded.abs();
    if !(1e-4..1e15).contains(&abs) {
        let s = format!("{:e}", rounded);
        let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let exp: i32 = exp.parse().unwrap_or(0);
        format!(
            "{}E{}{:02}",
            mantissa,
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        )
    } else {
        format!("{}", rounded)
    }
}

pub enum UsingArg {
    Num(f64),
    Str(String),
}

enum Field {
    Literal(char),
    FirstChar,
    WholeString,
    FixedString(usize),
    Number(NumField),
}

#[derive(Default)]
struct NumField {
    int_digits: usize,
    frac_digits: Option<usize>,
    comma: bool,
    lead_plus: bool,
    trail_plus: bool,
    trail_minus: bool,
    dollar: bool,
    asterisk: bool,
    exponent: bool,
}

fn parse_format(fmt: &str) -> Vec<Field> {
    let c: Vec<char> = fmt.chars().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    let at = |k: usize| c.get(k).copied().unwrap_or('\0');
    while i < c.len() {
        match c[i] {
            '_' if i + 1 < c.len() => {
                fields.push(Field::Literal(c[i + 1]));
                i += 2;
            }
            '!' => {
                fields.push(Field::FirstChar);
                i += 1;
            }
            '&' => {
                fields.push(Field::WholeString);
                i += 1;
            }
            '\\' => {
                let mut j = i + 1;
                while j < c.len() && c[j] == ' ' {
                    j += 1;
                }
                if j < c.len() && c[j] == '\\' {
                    fields.push(Field::FixedString(j - i + 1));
                    i = j + 1;
                } else {
                    fields.push(Field::Literal('\\'));
                    i += 1;
                }
            }
            _ => {
                let start_num = matches!(c[i], '#')
                    || (c[i] == '.' && at(i + 1) == '#')
                    || (c[i] == '+'
                        && (at(i + 1) == '#' || (at(i + 1) == '.' && at(i + 2) == '#')))
                    || (c[i] == '$' && at(i + 1) == '$')
                    || (c[i] == '*' && at(i + 1) == '*');
                if !start_num {
                    fields.push(Field::Literal(c[i]));
                    i += 1;
                    continue;
                }
                let mut f = NumField::default();
                if c[i] == '+' {
                    f.lead_plus = true;
                    i += 1;
                }
                if at(i) == '*' && at(i + 1) == '*' {
                    f.asterisk = true;
                    f.int_digits += 2;
                    i += 2;
                    if at(i) == '$' {
                        f.dollar = true;
                        i += 1;
                    }
                } else if at(i) == '$' && at(i + 1) == '$' {
                    f.dollar = true;
                    f.int_digits += 2;
                    i += 2;
                }
                while matches!(at(i), '#' | ',') {
                    if at(i) == ',' {
                        f.comma = true;
                    }
                    f.int_digits += 1;
                    i += 1;
                }
                if at(i) == '.' {
                    i += 1;
                    let mut d = 0;
                    while at(i) == '#' {
                        d += 1;
                        i += 1;
                    }
                    f.frac_digits = Some(d);
                }
                if c[i.min(c.len())..].starts_with(&['^', '^', '^', '^']) {
                    f.exponent = true;
                    i += 4;
                    if at(i) == '^' {
                        i += 1;
                    }
                }
                if !f.lead_plus {
                    if at(i) == '+' {
                        f.trail_plus = true;
                        i += 1;
                    } else if at(i) == '-' {
                        f.trail_minus = true;
                        i += 1;
                    }
                }
                fields.push(Field::Number(f));
            }
        }
    }
    fields
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn format_num_field(f: &NumField, value: f64) -> String {
    let negative = value < 0.0;
    let abs = value.abs();
    let frac = f.frac_digits.unwrap_or(0);
    let explicit_sign = f.lead_plus || f.trail_plus || f.trail_minus;
    let mut body;
    if f.exponent {
        let lead = f
            .int_digits
            .saturating_sub(if explicit_sign { 0 } else { 1 })
            .max(1);
        let (mut mantissa, mut exp) = if abs == 0.0 {
            (0.0, 0)
        } else {
            let e = abs.log10().floor() as i32 - (lead as i32 - 1);
            (abs / 10f64.powi(e), e)
        };
        let scale = 10f64.powi(frac as i32);
        if (mantissa * scale).round() / scale >= 10f64.powi(lead as i32) {
            mantissa /= 10.0;
            exp += 1;
        }
        body = format!(
            "{:.*}E{}{:02}",
            frac,
            mantissa,
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        );
        if f.frac_digits.is_some() && frac == 0 {
            body = body.replacen('E', ".E", 1);
        }
    } else {
        let s = format!("{:.*}", frac, abs);
        let (int_part, frac_part) = match s.split_once('.') {
            Some((a, b)) => (a.to_string(), Some(b.to_string())),
            None => (s, None),
        };
        let mut int_part = if f.comma {
            group_thousands(&int_part)
        } else {
            int_part
        };
        if int_part == "0" && f.int_digits == 0 && f.frac_digits.is_some() {
            int_part.clear();
        }
        body = int_part;
        if f.frac_digits.is_some() {
            body.push('.');
            body.push_str(frac_part.as_deref().unwrap_or(""));
        }
    }
    if f.dollar {
        body.insert(0, '$');
    }
    if f.lead_plus || (negative && !f.trail_plus && !f.trail_minus) {
        body.insert(0, if negative { '-' } else { '+' });
    }
    if f.trail_plus {
        body.push(if negative { '-' } else { '+' });
    } else if f.trail_minus {
        body.push(if negative { '-' } else { ' ' });
    }
    let width = f.int_digits
        + f.frac_digits.map(|d| d + 1).unwrap_or(0)
        + if f.exponent { 4 } else { 0 }
        + usize::from(f.lead_plus || f.trail_plus || f.trail_minus)
        + usize::from(f.dollar && f.asterisk);
    let len = body.chars().count();
    if len > width {
        format!("%{}", body)
    } else {
        let pad = if f.asterisk { '*' } else { ' ' };
        std::iter::repeat_n(pad, width - len).collect::<String>() + &body
    }
}

/// Formats values according to a `PRINT USING` template. The template is
/// reused when there are more values than fields; output stops at the first
/// field that has no value left.
pub fn print_using(fmt: &str, args: &[UsingArg]) -> Result<String, String> {
    let fields = parse_format(fmt);
    let has_field = fields.iter().any(|f| !matches!(f, Field::Literal(_)));
    if !has_field {
        if args.is_empty() {
            return Ok(fmt.to_string());
        }
        return Err("Illegal function call: PRINT USING format has no fields".to_string());
    }
    let mut out = String::new();
    let mut next = 0;
    'outer: loop {
        for field in &fields {
            match field {
                Field::Literal(ch) => out.push(*ch),
                other => {
                    let Some(arg) = args.get(next) else {
                        break 'outer;
                    };
                    next += 1;
                    match (other, arg) {
                        (Field::Number(f), UsingArg::Num(n)) => {
                            out.push_str(&format_num_field(f, *n))
                        }
                        (Field::FirstChar, UsingArg::Str(s)) => {
                            out.push(s.chars().next().unwrap_or(' '))
                        }
                        (Field::WholeString, UsingArg::Str(s)) => out.push_str(s),
                        (Field::FixedString(w), UsingArg::Str(s)) => {
                            let taken: String = s.chars().take(*w).collect();
                            let len = taken.chars().count();
                            out.push_str(&taken);
                            out.extend(std::iter::repeat_n(' ', w - len));
                        }
                        _ => return Err("Type mismatch in PRINT USING".to_string()),
                    }
                }
            }
        }
        if next >= args.len() {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_display_without_float_noise() {
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(1.0 / 3.0), "0.333333333333333");
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(-7.5), "-7.5");
        assert_eq!(format_number(1e300), "1E+300");
        assert_eq!(format_number(1.5e-7), "1.5E-07");
        assert_eq!(format_number(123456789012.0), "123456789012");
    }

    fn using(fmt: &str, args: Vec<UsingArg>) -> String {
        print_using(fmt, &args).unwrap()
    }

    #[test]
    fn print_using_numbers() {
        assert_eq!(using("##.##", vec![UsingArg::Num(4.56789)]), " 4.57");
        assert_eq!(using("###", vec![UsingArg::Num(5.0)]), "  5");
        assert_eq!(using("#,###", vec![UsingArg::Num(1234.0)]), "1,234");
        assert_eq!(using("$$##.##", vec![UsingArg::Num(5.5)]), "  $5.50");
        assert_eq!(using("**#.##", vec![UsingArg::Num(1.5)]), "**1.50");
        assert_eq!(using("##.##-", vec![UsingArg::Num(-2.0)]), " 2.00-");
        assert_eq!(using("+##", vec![UsingArg::Num(7.0)]), " +7");
        assert_eq!(using("#.#", vec![UsingArg::Num(123.0)]), "%123.0");
        assert_eq!(
            using("Total: ### items", vec![UsingArg::Num(12.0)]),
            "Total:  12 items"
        );
        assert_eq!(
            using("## ", vec![UsingArg::Num(1.0), UsingArg::Num(2.0)]),
            " 1  2 "
        );
    }

    #[test]
    fn print_using_strings() {
        assert_eq!(using("!", vec![UsingArg::Str("Hello".into())]), "H");
        assert_eq!(using("[&]", vec![UsingArg::Str("Hi".into())]), "[Hi]");
        assert_eq!(
            using("\\  \\|", vec![UsingArg::Str("abcdef".into())]),
            "abcd|"
        );
        assert_eq!(using("\\  \\|", vec![UsingArg::Str("ab".into())]), "ab  |");
    }
}
