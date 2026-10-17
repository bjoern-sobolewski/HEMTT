use std::ops::Range;
use std::sync::Arc;

use chumsky::prelude::*;
use chumsky::text::ident;

use crate::StringWrapper;

pub type Tokens = Vec<(Token, Range<usize>)>;

macro_rules! chain_collect {
    ($Collect:ty: $($value:expr),+ $(,)?) => {
        std::iter::empty()$(.chain($value))+.collect::<$Collect>()
    };
}

pub fn strip_comments(tokens: &mut Tokens) {
    tokens.retain(|(token, _)| !matches!(token, Token::Comment(..)));
}

pub fn strip_noop(tokens: &mut Tokens) {
    let mut last_flag_delete = true;
    tokens.retain(|(token, _)| {
        let delete = matches!(token, Token::Control(Control::Terminator)) && last_flag_delete;
        last_flag_delete = matches!(
            token,
            Token::Control(Control::Terminator | Control::CurlyBracketOpen)
        );
        !delete
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    /// Single-line or multi-line comment.
    /// Contains the entire contents of the comment.
    Comment(String),
    /// A pre-processor macro.
    Macro(String, String),
    /// An operator.
    Operator(Operator),
    /// A special control character.
    Control(Control),
    /// A scalar number literal.
    Number(crate::Scalar<f32>),
    /// An identifier, keyword, or command.
    Identifier(String),
    /// A string literal.
    String(Arc<str>, StringWrapper),
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Terminator,
    Separator,
    SquareBracketOpen,
    SquareBracketClose,
    RoundBracketOpen,
    RoundBracketClose,
    CurlyBracketOpen,
    CurlyBracketClose,
}

impl Control {
    #[must_use]
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Terminator => ";",
            Self::Separator => ",",
            Self::SquareBracketOpen => "[",
            Self::SquareBracketClose => "]",
            Self::RoundBracketOpen => "(",
            Self::RoundBracketClose => ")",
            Self::CurlyBracketOpen => "{",
            Self::CurlyBracketClose => "}",
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    Not,
    Or,
    And,
    Eq,
    NotEq,
    Greater,
    Less,
    GreaterEq,
    LessEq,
    ConfigPath,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Exp,
    /// Equals `=`
    Assign,
    /// Colon `:`
    Associate,
    /// Pound sign `#`
    Select,
}

impl Operator {
    #[must_use]
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::And => "&&",
            Self::Or => "||",
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::ConfigPath => ">>",
            Self::GreaterEq => ">=",
            Self::LessEq => "<=",
            Self::Greater => ">",
            Self::Less => "<",
            Self::Not => "!",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Exp => "^",
            Self::Assign => "=",
            Self::Associate => ":",
            Self::Select => "#",
        }
    }
}

/// Lexes a raw SQF file into a token list, to be passed onto the parser stage.
///
/// # Errors
/// Returns chumsky's [`Simple`] error type, which is a wrapper around [`Range<usize>`] and [`String`].
pub fn run(input: impl AsRef<str>) -> Result<Tokens, Vec<Simple<char>>> {
    let input = input.as_ref();
    // Profiling control for sequential equivalence/performance workloads. The
    // same released binary can exercise the retained reference implementation.
    static REFERENCE: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var_os("HEMTT_REFERENCE_LEXER").is_some());
    if *REFERENCE {
        return lexer().parse(input);
    }
    fast_tokens(input).map_or_else(|| lexer().parse(input), Ok)
}

// Dispatch common tokens by their first byte instead of constructing and
// merging unsuccessful combinator branches for every character. Unusual or
// malformed input uses the original grammar for identical error diagnostics.
fn fast_tokens(input: &str) -> Option<Tokens> {
    let bytes = input.as_bytes();
    let mut offset = 0;
    let mut character = 0;
    let mut tokens = Vec::new();
    while offset < bytes.len() {
        let start = offset;
        let ch = input[offset..].chars().next()?;
        if ch.is_whitespace() {
            offset += ch.len_utf8();
            character += 1;
            continue;
        }
        let token = match bytes[offset] {
            b'\'' | b'"' => {
                let quote = bytes[offset];
                offset += 1;
                let mut text = String::new();
                loop {
                    let ch = input[offset..].chars().next()?;
                    offset += ch.len_utf8();
                    if ch as u32 == u32::from(quote) {
                        if bytes.get(offset) == Some(&quote) {
                            offset += 1;
                            text.push(ch);
                        } else {
                            break;
                        }
                    } else {
                        text.push(ch);
                    }
                }
                Token::String(
                    text.into(),
                    if quote == b'\'' {
                        StringWrapper::SingleQuote
                    } else {
                        StringWrapper::DoubleQuote
                    },
                )
            }
            b'/' if bytes.get(offset + 1) == Some(&b'/') => {
                offset += 2;
                while offset < bytes.len()
                    && bytes[offset] != b'\n'
                    && !(bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n'))
                {
                    offset += 1;
                }
                Token::Comment(input[start..offset].into())
            }
            b'/' if bytes.get(offset + 1) == Some(&b'*') => {
                offset += 2;
                offset += input[offset..].find("*/")? + 2;
                Token::Comment(input[start..offset].into())
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                offset += 1;
                while bytes
                    .get(offset)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    offset += 1;
                }
                // The reference identifier accepts Unicode; do not split it.
                if bytes.get(offset).is_some_and(|b| !b.is_ascii()) {
                    return None;
                }
                Token::Identifier(input[start..offset].into())
            }
            b'0'..=b'9' | b'$' | b'.' => {
                let hex = bytes[offset] == b'$' || input[offset..].starts_with("0x");
                if hex {
                    offset += if bytes[offset] == b'$' { 1 } else { 2 };
                    let digits = offset;
                    while bytes.get(offset).is_some_and(u8::is_ascii_hexdigit) {
                        offset += 1;
                    }
                    if offset == digits {
                        return None;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let number = u64::from_str_radix(&input[digits..offset], 16).ok()? as f32;
                    Token::Number(crate::Scalar(number))
                } else {
                    while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
                        offset += 1;
                    }
                    let mut float = false;
                    if bytes.get(offset) == Some(&b'.')
                        && bytes.get(offset + 1).is_some_and(u8::is_ascii_digit)
                    {
                        float = true;
                        offset += 1;
                        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
                            offset += 1;
                        }
                    }
                    if offset == start {
                        return None;
                    }
                    if bytes[start] != b'.'
                        && bytes.get(offset).is_some_and(|b| matches!(b, b'e' | b'E'))
                    {
                        let mut end = offset + 1;
                        if bytes.get(end).is_some_and(|b| matches!(b, b'+' | b'-')) {
                            end += 1;
                        }
                        let digits = end;
                        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                            end += 1;
                        }
                        if end != digits {
                            float = true;
                            offset = end;
                        }
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let number = if float {
                        input[start..offset].parse::<f32>().ok()?
                    } else {
                        input[start..offset].parse::<u64>().ok()? as f32
                    };
                    Token::Number(crate::Scalar(number))
                }
            }
            _ => {
                let pair = bytes.get(offset..offset + 2);
                let operator = match pair {
                    Some(b"&&") => Some(Operator::And),
                    Some(b"||") => Some(Operator::Or),
                    Some(b"==") => Some(Operator::Eq),
                    Some(b"!=") => Some(Operator::NotEq),
                    Some(b">>") => Some(Operator::ConfigPath),
                    Some(b">=") => Some(Operator::GreaterEq),
                    Some(b"<=") => Some(Operator::LessEq),
                    _ => None,
                };
                if let Some(operator) = operator {
                    offset += 2;
                    Token::Operator(operator)
                } else {
                    offset += 1;
                    match bytes[start] {
                        b';' => Token::Control(Control::Terminator),
                        b',' => Token::Control(Control::Separator),
                        b'[' => Token::Control(Control::SquareBracketOpen),
                        b']' => Token::Control(Control::SquareBracketClose),
                        b'(' => Token::Control(Control::RoundBracketOpen),
                        b')' => Token::Control(Control::RoundBracketClose),
                        b'{' => Token::Control(Control::CurlyBracketOpen),
                        b'}' => Token::Control(Control::CurlyBracketClose),
                        b'>' => Token::Operator(Operator::Greater),
                        b'<' => Token::Operator(Operator::Less),
                        b'!' => Token::Operator(Operator::Not),
                        b'+' => Token::Operator(Operator::Add),
                        b'-' => Token::Operator(Operator::Sub),
                        b'*' => Token::Operator(Operator::Mul),
                        b'/' => Token::Operator(Operator::Div),
                        b'%' => Token::Operator(Operator::Rem),
                        b'^' => Token::Operator(Operator::Exp),
                        b'=' => Token::Operator(Operator::Assign),
                        b':' => Token::Operator(Operator::Associate),
                        b'#' => Token::Operator(Operator::Select),
                        _ => return None,
                    }
                }
            }
        };
        let end = character + input[start..offset].chars().count();
        tokens.push((token, character..end));
        character = end;
    }
    Some(tokens)
}

fn lexer() -> impl Parser<char, Tokens, Error = Simple<char>> {
    end().padded().map(|()| vec![]).or(comment()
        .or(base())
        .map_with_span(|token, span| (token, span))
        .padded()
        .repeated()
        .then_ignore(end()))
}

fn base() -> impl Parser<char, Token, Error = Simple<char>> {
    let number = number().map(Token::Number);
    let identifier = ident().map(Token::Identifier);
    let string_single = string('\'').map(|s| Token::String(s.into(), StringWrapper::SingleQuote));
    let string_double = string('\"').map(|s| Token::String(s.into(), StringWrapper::DoubleQuote));

    choice((
        operator().map(Token::Operator),
        control().map(Token::Control),
        // Quotes delimit tokens without whitespace in native SQF (isNil"_x",
        // for"_i"from). The parser still rejects missing operators/arguments.
        choice((number, identifier, string_single, string_double)),
    ))
}

/// Captures comments
fn comment() -> impl Parser<char, Token, Error = Simple<char>> {
    choice((
        multi_line_comment().map(Token::Comment),
        single_line_comment().map(Token::Comment),
    ))
}

fn multi_line_comment() -> impl Parser<char, String, Error = Simple<char>> {
    just("/*").then(just("*/").not().repeated()).then(just("*/"))
    .map(|((prefix, comment), suffix)| {
        chain_collect!(String: prefix.chars(), comment, suffix.chars())
    })
}

fn single_line_comment() -> impl Parser<char, String, Error = Simple<char>> {
    just("//")
        .then(newline().not().repeated())
        .map(|(prefix, comment)| chain_collect!(String: prefix.chars(), comment))
}

fn newline() -> impl Parser<char, (), Error = Simple<char>> {
    just("\r\n").or(just("\n")).ignored()
}

#[allow(clippy::cast_precision_loss)]
fn number() -> impl Parser<char, crate::Scalar<f32>, Error = Simple<char>> {
    choice((
        number_float_exponent(),
        number_float_basic(),
        number_hex().map(|value| value as f32),
        number_int().map(|value| value as f32),
    ))
    .map(crate::Scalar)
}

fn number_hex() -> impl Parser<char, u64, Error = Simple<char>> {
    let digits = one_of("0123456789abcdefABCDEF").repeated().at_least(1);
    just("$")
        .or(just("0x"))
        .ignore_then(digits)
        .collect::<String>()
        .map(|value| u64::from_str_radix(&value, 16))
        .try_map(error_map)
}

fn number_int() -> impl Parser<char, u64, Error = Simple<char>> {
    number_digits()
        .collect::<String>()
        //.then_ignore(just('.').not().rewind())
        .from_str::<u64>()
        .try_map(error_map)
}

fn number_float_exponent() -> impl Parser<char, f32, Error = Simple<char>> {
    number_digits()
        .chain(just('.'))
        .or_not()
        .chain::<char, _, _>(number_digits())
        .chain::<char, _, _>(one_of("eE"))
        .chain::<char, _, _>(one_of("-+").or_not())
        .chain::<char, _, _>(number_digits())
        .collect::<String>()
        .from_str::<f32>()
        .try_map(error_map)
}

fn number_float_basic() -> impl Parser<char, f32, Error = Simple<char>> {
    number_digits()
        .or_not()
        .chain::<char, _, _>(just('.'))
        .chain::<char, _, _>(number_digits())
        .collect::<String>()
        .from_str::<f32>()
        .try_map(error_map)
}

fn number_digits() -> impl Parser<char, Vec<char>, Error = Simple<char>> {
    one_of("0123456789").repeated().at_least(1)
}

fn string(delimiter: char) -> impl Parser<char, String, Error = Simple<char>> {
    let content = just(delimiter).not().or(just([delimiter; 2]).to(delimiter));
    just(delimiter)
        .ignore_then(content.repeated())
        .then_ignore(just(delimiter))
        .collect()
}

fn operator() -> impl Parser<char, Operator, Error = Simple<char>> + Copy {
    choice((
        just("&&").to(Operator::And),
        just("||").to(Operator::Or),
        just("==").to(Operator::Eq),
        just("!=").to(Operator::NotEq),
        just(">>").to(Operator::ConfigPath),
        just(">=").to(Operator::GreaterEq),
        just("<=").to(Operator::LessEq),
        just(">").to(Operator::Greater),
        just("<").to(Operator::Less),
        just("!").to(Operator::Not),
        just("+").to(Operator::Add),
        just("-").to(Operator::Sub),
        just("*").to(Operator::Mul),
        just("/").to(Operator::Div),
        just("%").to(Operator::Rem),
        just("^").to(Operator::Exp),
        just("=").to(Operator::Assign),
        just(":").to(Operator::Associate),
        just("#").to(Operator::Select),
    ))
}

fn control() -> impl Parser<char, Control, Error = Simple<char>> + Copy {
    choice((
        just(";").to(Control::Terminator),
        just(",").to(Control::Separator),
        just("[").to(Control::SquareBracketOpen),
        just("]").to(Control::SquareBracketClose),
        just("(").to(Control::RoundBracketOpen),
        just(")").to(Control::RoundBracketClose),
        just("{").to(Control::CurlyBracketOpen),
        just("}").to(Control::CurlyBracketClose),
    ))
}

#[inline]
fn error_map<T, E, I>(result: Result<T, E>, span: Range<usize>) -> Result<T, Simple<I>>
where
    E: ToString,
    I: std::hash::Hash + Eq,
{
    result.map_err(|err| Simple::custom(span, err))
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    #[test]
    fn dispatch_matches_reference_tokens_and_character_spans() {
        let pieces = [
            "foo",
            "_a12",
            "42",
            "0xFF",
            "$ff",
            ".25",
            "12.5",
            "1e-3",
            "1.25E+4",
            "18446744073709551616",
            "1e",
            "0x",
            "12.",
            "'a''b'",
            "\"😀ä\"",
            "\"\"",
            "\"\"\"\"",
            "'unclosed",
            "// comment\r\n",
            "/*ä*/",
            "/*open",
            "\u{2003}",
            "äfoo",
            "α",
            "&&",
            "&",
            "||",
            "==",
            "!=",
            ">>",
            ">=",
            "<=",
            "!",
            "+",
            "-",
            "*",
            "/",
            "%",
            "^",
            "=",
            ":",
            "#",
            ";",
            ",",
            "[",
            "]",
            "(",
            ")",
            "{",
            "}",
            "\n",
        ];
        let mut seed = 0x734a_u64;
        for _ in 0..12000 {
            let mut source = String::new();
            for _ in 0..6 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                source.push_str(pieces[(seed as usize >> 16) % pieces.len()]);
                if seed & 1 != 0 {
                    source.push(' ');
                }
            }
            if let Some(tokens) = fast_tokens(&source) {
                assert_eq!(
                    Ok(tokens),
                    lexer().parse(source.as_str()),
                    "source: {source:?}"
                );
            }
        }
    }
}
