use memchr::memmem;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Encounted an illegal control character in a comment.
    ControlCharacter {
        pos: usize,
    },
    /// A string contains too many consececutive quote characters.
    TooManyQuotesInString {
        start: usize,
        pos: usize,
    },
    /// String was not terminated.
    UnterminatedString {
        start: usize,
        pos: usize,
    },
    /// A newline was encountered while parsing a key.
    MultilineKey {
        pos: usize,
    },
    /// A multi-line literal string or basic string was encountered where it's not allowed (e.g. in a table).
    MultilineString {
        pos: usize,
    },
    /// Expected a delimiter between elements when parsing an array or inline table.
    MissingDelimiter {
        pos: usize,
    },
    /// The parser did not consume the entire input string.
    UnconsumedInput {
        pos: usize,
    },
    Expected {
        pos: usize,
        c: char,
    },
    Unexpected {
        pos: usize,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    lo: usize,
    hi: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sym {
    Eof,
    Table,
    ArrayOfTable,
    TableEnd,
    InlineTable,
    InlineTableEnd,
    Array,
    ArrayEnd,
    Assign,
    Key,
    String,
    Integer,
    Float,
    Bool,
    DateTime,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    sym: Sym,
    span: Span,
}

impl Symbol {
    pub const fn new(sym: Sym, pos: usize) -> Self {
        Symbol {
            sym,
            span: Span {
                lo: pos,
                hi: pos + 1,
            },
        }
    }

    pub const fn with_span(sym: Sym, lo: usize, hi: usize) -> Self {
        Symbol {
            sym,
            span: Span { lo, hi: hi + 1 },
        }
    }

    pub fn sym(&self) -> Sym {
        self.sym
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

pub struct Lex<'a> {
    pub text: &'a str,
    index: usize,
    current: u8,
    pub symbols: Vec<Symbol>,

    #[cfg(test)]
    pub crash_on_error: bool,
}

impl<'a> Lex<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            index: 0,
            current: *text.as_bytes().get(0).unwrap_or(&0),
            symbols: Vec::new(),

            #[cfg(test)]
            crash_on_error: false,
        }
    }

    #[inline(always)]
    fn eat(&mut self, c: u8) -> bool {
        if self.current == c {
            self.next();
            true
        } else {
            false
        }
    }

    #[inline(always)]
    fn next(&mut self) {
        self.index += 1;
        self.current = *self.text.as_bytes().get(self.index).unwrap_or(&0);
        //println!("{}", self.current as char);
    }

    #[inline(always)]
    fn peek(&self) -> u8 {
        *self.text.as_bytes().get(self.index + 1).unwrap_or(&0)
    }

    #[inline(always)]
    fn advance(&mut self, index: usize) {
        self.index = index;
        self.current = *self.text.as_bytes().get(self.index).unwrap_or(&0);
    }

    #[inline(always)]
    fn push(&mut self, sym: Sym) {
        self.symbols.push(Symbol::new(sym, self.index))
    }

    /// Pushes a symbol with the given range. Note that hi is given as the index of the final
    /// character included in the span, however it's stored as one past the final index.
    #[inline(always)]
    fn push_span(&mut self, sym: Sym, lo: usize, hi: usize) {
        self.symbols.push(Symbol::with_span(sym, lo, hi))
    }

    #[cold]
    fn err(&self, e: Error) -> Result<(), Error> {
        #[cfg(test)]
        if self.crash_on_error {
            panic!("err {}", e)
        }
        Err(e)
    }

    #[cold]
    fn err_unterminated_string(&self, start: usize) -> Result<(), Error> {
        self.err(Error::UnterminatedString {
            start,
            pos: self.index,
        })
    }

    #[cold]
    fn err_too_many_quotes_in_string(&self, start: usize) -> Result<(), Error> {
        self.err(Error::TooManyQuotesInString {
            start,
            pos: self.index,
        })
    }

    #[cold]
    fn err_illegal_control_character(&self) -> Result<(), Error> {
        self.err(Error::ControlCharacter { pos: self.index })
    }

    #[cold]
    fn err_multiline_key(&self) -> Result<(), Error> {
        self.err(Error::MultilineKey { pos: self.index })
    }

    #[cold]
    fn err_illegal_multiline_string(&self) -> Result<(), Error> {
        self.err(Error::MultilineString { pos: self.index })
    }

    #[cold]
    fn err_unconsumed_input(&self) -> Result<(), Error> {
        self.err(Error::UnconsumedInput { pos: self.index })
    }

    #[cold]
    fn err_expected(&self, c: u8) -> Result<(), Error> {
        self.err(Error::Expected {
            pos: self.index,
            c: c as char,
        })
    }

    #[cold]
    fn err_unexpected(&self) -> Result<(), Error> {
        self.err(Error::Unexpected { pos: self.index })
    }

    #[cold]
    fn err_missing_delimiter(&self) -> Result<(), Error> {
        self.err(Error::MissingDelimiter { pos: self.index })
    }

    /// Consumes a comment until the end of line or end of file.
    fn consume_comment(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'#');
        self.next();

        loop {
            match self.current {
                0 | b'\n' => break,
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.next();
                    self.next();
                    break;
                }
                0x1..=0x8 | 0xa..=0x1f | 0x7f => self.err_illegal_control_character()?,
                _ => self.next(),
            }
        }
        Ok(())
    }

    /// Consumes bytes until the next newline, or end of file, including comments.
    fn consume_line(&mut self) -> Result<(), Error> {
        loop {
            match self.current {
                0 | b'\n' => break,
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.next();
                    self.next();
                    break;
                }
                b' ' | b'\t' => self.next(),
                b'#' => break self.consume_comment()?,
                _ => self.err_unexpected()?,
            }
        }
        Ok(())
    }

    /// Skip all forms of whitespace including comments and newlines.
    fn skip_whitespace_and_comment(&mut self) -> Result<(), Error> {
        loop {
            match self.current {
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.next();
                    self.next();
                    break;
                }
                b'\n' | b' ' | b'\t' => self.next(),
                b'#' => self.consume_comment()?,
                _ => break,
            }
        }
        Ok(())
    }

    /// Skip only spaces and tabs. Stops on comments and newlines.
    fn skip_whitespace(&mut self) -> Result<(), Error> {
        loop {
            match self.current {
                b' ' | b'\t' => self.next(),
                _ => break,
            }
        }
        Ok(())
    }

    fn scan_key(&mut self) -> Result<(), Error> {
        let start = self.index;
        loop {
            self.next();
            match self.current {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {}
                b' ' | b'\t' | b'=' | b'.' | b']' => break,
                _ => self.err_unexpected()?,
            }
        }
        self.push_span(Sym::Key, start, self.index - 1);
        Ok(())
    }

    fn scan_multiline_basic_string(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'"');
        self.next();
        debug_assert_eq!(self.current, b'"');
        self.next();
        debug_assert_eq!(self.current, b'"');
        self.next();

        let start = self.index;

        let mut quote_count = 0;
        let mut slash_count = 0;
        loop {
            match self.current {
                b'"' => {
                    if slash_count & 1 == 0 {
                        quote_count += 1;
                    } else {
                        quote_count = 0;
                    }
                    slash_count = 0;
                }
                b'\\' => {
                    match quote_count {
                        0 | 1 | 2 => {}
                        3 | 4 | 5 => {
                            let extra = quote_count - 3;
                            break self.push_span(Sym::String, start, self.index - extra);
                        }
                        _ => self.err_too_many_quotes_in_string(start)?,
                    }

                    slash_count += 1;
                    quote_count = 0;
                }
                ch @ _ => {
                    match quote_count {
                        0 | 1 | 2 => {
                            if ch == 0 {
                                self.err_unterminated_string(start)?;
                            }
                        }
                        3 | 4 | 5 => {
                            let extra = quote_count - 3;
                            break self.push_span(Sym::String, start, self.index - extra);
                        }
                        _ => self.err_too_many_quotes_in_string(start)?,
                    }
                    quote_count = 0;
                }
            }

            self.next();
        }

        Ok(())
    }

    fn scan_basic_string(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'"');
        self.next();

        let start = self.index;

        let mut slash_count = 0;
        loop {
            match self.current {
                b'"' => {
                    if slash_count & 1 == 0 {
                        break;
                    }
                    slash_count = 0;
                }
                b'\\' => slash_count += 1,
                b'\n' | 0 => self.err_unterminated_string(start)?,
                _ => slash_count = 0,
            }
            self.next();
        }

        self.push_span(Sym::String, start, self.index);
        self.next();
        Ok(())
    }

    fn scan_multiline_literal_string(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'\'');
        self.next();
        debug_assert_eq!(self.current, b'\'');
        self.next();
        debug_assert_eq!(self.current, b'\'');
        self.next();

        let start = self.index;
        let rest = &self.text.as_bytes()[start..];

        if let Some(index) = memmem::find(rest, &[b'\'', b'\'', b'\'']) {
            self.advance(start + index + 3);
            if self.eat(b'\'') {
                if self.eat(b'\'') {
                    if self.current == b'\'' {
                        self.err_too_many_quotes_in_string(start)?;
                    }
                }
            }
            self.push_span(Sym::String, start, self.index - 3);
            Ok(())
        } else {
            self.err_unterminated_string(start)
        }
    }

    fn scan_literal_string(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'\'');
        self.next();

        let start = self.index;
        let rest = &self.text.as_bytes()[start..];
        if let Some(index) = memchr::memchr3(b'\n', b'\'', b'\0', rest) {
            self.advance(start + index + 1);
            if rest[index] == b'\'' {
                self.push_span(Sym::String, start, start + index - 1);
                return Ok(());
            }
        }
        self.err_unterminated_string(start)
    }

    fn scan_string(&mut self) -> Result<(), Error> {
        match &self.text.as_bytes()[self.index..] {
            [b'\'', b'\'', b'\'', ..] => self.scan_multiline_literal_string(),
            [b'"', b'"', b'"', ..] => self.scan_multiline_basic_string(),
            [b'\'', ..] => self.scan_literal_string(),
            [b'"', ..] => self.scan_basic_string(),
            _ => panic!(),
        }
    }

    fn scan_single_line_string(&mut self) -> Result<(), Error> {
        match &self.text.as_bytes()[self.index..] {
            [b'\'', b'\'', b'\'', ..] => self.err_illegal_multiline_string(),
            [b'"', b'"', b'"', ..] => self.err_illegal_multiline_string(),
            [b'\'', ..] => self.scan_literal_string(),
            [b'"', ..] => self.scan_basic_string(),
            _ => panic!(),
        }
    }

    fn scan_number(&mut self) -> Result<(), Error> {
        let start = self.index;

        match self.current {
            b'-' | b'+' => self.next(),
            _ => {}
        };

        if self.current == b'0' {
            match self.peek() {
                b'x' | b'X' => {
                    self.next();
                    self.next();
                    let mut allow_underscore = false;
                    loop {
                        match self.current {
                            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                                allow_underscore = true;
                                self.next();
                            }
                            b'_' => {
                                if !allow_underscore {
                                    self.err_unexpected()?;
                                }
                                allow_underscore = false;
                            }
                            0 | b' ' | b'\t' | b'\n' | b'#' | b',' => break,
                            _ => self.err_unexpected()?,
                        }
                    }
                    self.push_span(Sym::Integer, start, self.index - 1);
                    return Ok(());
                }
                b'o' | b'O' => {
                    self.next();
                    self.next();
                    let mut allow_underscore = false;
                    loop {
                        match self.current {
                            b'0'..=b'7' => {
                                allow_underscore = true;
                                self.next();
                            }
                            b'_' => {
                                if !allow_underscore {
                                    self.err_unexpected()?;
                                }
                                allow_underscore = false;
                                self.next();
                            }
                            0 | b' ' | b'\t' | b'\n' | b'#' | b',' => break,
                            _ => self.err_unexpected()?,
                        }
                    }
                    self.push_span(Sym::Integer, start, self.index - 1);
                    return Ok(());
                }
                b'b' | b'B' => {
                    self.next();
                    self.next();
                    let mut allow_underscore = false;
                    loop {
                        match self.current {
                            b'0' | b'1' => {
                                allow_underscore = true;
                                self.next();
                            }
                            b'_' => {
                                if !allow_underscore {
                                    self.err_unexpected()?;
                                }
                                allow_underscore = false;
                                self.next();
                            }
                            0 | b' ' | b'\t' | b'\n' | b'#' | b',' => break,
                            _ => self.err_unexpected()?,
                        }
                    }
                    self.push_span(Sym::Integer, start, self.index - 1);
                    return Ok(());
                }
                _ => {}
            }
        }

        let mut allow_underscore = false;
        loop {
            match self.current {
                b'0'..=b'9' => {
                    allow_underscore = true;
                    self.next();
                }
                b'_' => {
                    if !allow_underscore {
                        self.err_unexpected()?;
                    }
                    allow_underscore = false;
                    self.next();
                }
                0 | b' ' | b'\t' | b'\n' | b'#' | b',' => break,
                _ => self.err_unexpected()?,
            }
        }
        self.push_span(Sym::Integer, start, self.index - 1);

        Ok(())
    }

    fn scan_number_or_date(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn scan_array(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'[');

        self.push(Sym::Array);
        self.next();

        self.skip_whitespace_and_comment()?;

        if self.current == b']' {
            self.push(Sym::ArrayEnd);
            self.next();
            return Ok(());
        }

        self.scan_value()?;

        loop {
            self.skip_whitespace_and_comment()?;
            match self.current {
                b',' => {
                    self.next();
                    self.skip_whitespace_and_comment()?;
                    if self.current == b']' {
                        break;
                    }
                    self.scan_value()?;
                }
                b']' => break,
                _ => self.err_missing_delimiter()?,
            }
        }

        self.push(Sym::ArrayEnd);
        self.next();

        Ok(())
    }

    fn scan_inline_table(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'{');

        self.push(Sym::InlineTable);
        self.next();

        self.skip_whitespace()?;

        if self.eat(b'}') {
            self.push(Sym::InlineTableEnd);
            return Ok(());
        }

        self.scan_key_like()?;
        self.skip_whitespace()?;
        self.scan_value()?;

        loop {
            self.skip_whitespace()?;
            match self.current {
                b',' => {
                    self.next();
                    self.skip_whitespace()?;
                    self.scan_key_like()?;
                    self.skip_whitespace()?;
                    self.scan_value()?;
                }
                b'}' => break,
                _ => self.err_missing_delimiter()?,
            }
        }

        self.push(Sym::InlineTableEnd);
        self.next();
        Ok(())
    }

    /// Consumes the remainder of a key-like after the first key or string up to the '=' character.
    fn scan_dotted(&mut self) -> Result<(), Error> {
        let mut saw_dot = false;
        loop {
            self.skip_whitespace()?;
            match self.current {
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.err_multiline_key()?
                }
                b'\n' => self.err_multiline_key()?,
                b'.' => {
                    if saw_dot {
                        self.err_unexpected()?;
                    }
                    saw_dot = true;
                    self.next();
                }
                b'=' => {
                    if saw_dot {
                        self.err_unexpected()?;
                    }
                    break;
                }
                b'"' | b'\'' => {
                    if !saw_dot {
                        self.err_unexpected()?
                    }
                    saw_dot = false;
                    self.scan_string()?
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    if !saw_dot {
                        self.err_unexpected()?
                    }
                    saw_dot = false;
                    self.scan_key()?
                }
                _ => self.err_unexpected()?,
            }
        }

        // Eat the '=' as well since we do that on all paths.
        self.push(Sym::Assign);
        self.next();

        Ok(())
    }

    /// Scan an entire key-like up to the '=' character.
    fn scan_key_like(&mut self) -> Result<(), Error> {
        match self.current {
            b'"' | b'\'' => self.scan_string()?,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => self.scan_key()?,
            _ => self.err_unexpected()?,
        }
        self.scan_dotted()?;
        Ok(())
    }

    fn scan_value(&mut self) -> Result<(), Error> {
        match self.current {
            b'"' | b'\'' => self.scan_string()?,
            b'{' => self.scan_inline_table()?,
            b'[' => self.scan_array()?,
            b't' | b'f' => match &self.text.as_bytes()[self.index..] {
                &[b't', b'r', b'u', b'e', ..] => {
                    let start = self.index;
                    let end = self.index + 4;
                    self.advance(end);
                    self.push_span(Sym::Bool, start, end);
                }
                &[b'f', b'a', b'l', b's', b'e', ..] => {
                    let start = self.index;
                    let end = self.index + 5;
                    self.advance(end);
                    self.push_span(Sym::Bool, start, end);
                }
                _ => self.err_unexpected()?,
            },
            b'i' | b'n' => match &self.text.as_bytes()[self.index..] {
                &[b'i', b'n', b'f'] | &[b'n', b'a', b'n'] => {
                    let start = self.index;
                    let end = self.index + 3;
                    self.advance(end);
                    self.push_span(Sym::Float, start, end);
                }
                _ => self.err_unexpected()?,
            },
            b'+' | b'-' => self.scan_number()?,
            b'0'..=b'9' => self.scan_number_or_date()?,
            _ => self.err_unexpected()?,
        }

        Ok(())
    }

    fn scan_table(&mut self) -> Result<(), Error> {
        debug_assert_eq!(self.current, b'[');
        self.next();

        let is_array = self.eat(b'[');

        self.push(if is_array {
            Sym::ArrayOfTable
        } else {
            Sym::Table
        });

        let mut saw_dot = true;
        loop {
            match self.current {
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.err_multiline_key()?
                }
                b'\n' => self.err_multiline_key()?,
                b' ' | b'\t' => self.next(),
                b'.' => {
                    if saw_dot {
                        self.err_unexpected()?;
                    }
                    saw_dot = true;
                    self.next();
                }
                b']' => {
                    if saw_dot {
                        self.err_unexpected()?;
                    }
                    self.next();
                    break;
                }
                b'"' | b'\'' => {
                    if !saw_dot {
                        self.err_unexpected()?
                    }
                    saw_dot = false;
                    self.scan_single_line_string()?
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    if !saw_dot {
                        self.err_unexpected()?
                    }
                    saw_dot = false;
                    self.scan_key()?
                }
                _ => self.err_unexpected()?,
            }
        }

        if is_array && !self.eat(b']') {
            self.err_expected(b']')?;
        }

        self.consume_line()?;

        self.push(Sym::TableEnd);

        Ok(())
    }

    pub fn scan(&mut self) -> Result<(), Error> {
        loop {
            match self.current {
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.next();
                    self.next();
                }
                b'\n' | b' ' | b'\t' => self.next(),
                b'#' => self.consume_comment()?,
                b'[' => self.scan_table()?,
                b'"' | b'\'' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    self.scan_key_like()?;
                    self.skip_whitespace()?;
                    self.scan_value()?;
                    self.consume_line()?;
                }
                0 => break,
                _ => self.err_unexpected()?,
            }
        }

        if self.index != self.text.len() {
            self.err_unconsumed_input()?;
        }

        self.push(Sym::Eof);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fail {
        ($text: expr, $err: expr) => {
            let mut lex = Lex::new($text);
            let res = lex.scan();
            let err = Err($err);
            if res != err {
                panic!(
                    "expected error:\nexpected: {}\n   found: {}",
                    format!("{:?}", err),
                    format!("{:?}", res)
                )
            }
        };
    }

    macro_rules! succ {
        ($text: expr, $syms: expr) => {
            let mut lex = Lex::new($text);
            lex.crash_on_error = true;
            lex.scan().expect("parse failed");
            let expect_syms: &[Symbol] = $syms;
            let got_syms = &lex.symbols[..];
            if expect_syms != got_syms {
                let max_len = got_syms.len().max(expect_syms.len());
                for i in 0..max_len {
                    let (expected, found) = (expect_syms.get(i), got_syms.get(i));
                    if expected == found {
                        eprintln!("{:<4} {}", i, format!("{:?}", expected));
                    } else {
                        eprintln!("{:<4} expected: {}", i, format!("{:?}", expected),);
                        eprintln!("{:<4}    found: {}", " ", format!("{:?}", found))
                    }
                }
                panic!("assertion failed")
            }
        };
    }

    #[test]
    fn basic_fail() {
        fail!("=", Error::Unexpected { pos: 0 });
        fail!("\0", Error::UnconsumedInput { pos: 0 });
    }

    #[test]
    fn basic_success() {
        succ!("", &[Symbol::new(Sym::Eof, 0)]);
        succ!(
            "hello = 'world'",
            &[
                Symbol::with_span(Sym::Key, 0, 4),
                Symbol::new(Sym::Assign, 6),
                Symbol::with_span(Sym::String, 9, 13),
                Symbol::new(Sym::Eof, 15),
            ]
        );
        succ!(
            "hello = 'world' # zing bing bang",
            &[
                Symbol::with_span(Sym::Key, 0, 4),
                Symbol::new(Sym::Assign, 6),
                Symbol::with_span(Sym::String, 9, 13),
                Symbol::new(Sym::Eof, 32),
            ]
        );
        succ!(
            "hello = true",
            &[
                Symbol::with_span(Sym::Key, 0, 4),
                Symbol::new(Sym::Assign, 6),
                Symbol::with_span(Sym::Bool, 8, 12),
                Symbol::new(Sym::Eof, 12),
            ]
        );
        succ!(
            "hello = false",
            &[
                Symbol::with_span(Sym::Key, 0, 4),
                Symbol::new(Sym::Assign, 6),
                Symbol::with_span(Sym::Bool, 8, 13),
                Symbol::new(Sym::Eof, 13),
            ]
        );
    }

    #[test]
    fn comment_fail() {
        fail!("# He\u{1}llo,\n# World", Error::ControlCharacter { pos: 4 });
        fail!("# He\rllo,\r\n# World!", Error::ControlCharacter { pos: 4 });
    }

    #[test]
    fn comment_success() {
        succ!("# Hello,\n# World!", &[Symbol::new(Sym::Eof, 17)]);
        succ!("# Hello,\r\n# World!", &[Symbol::new(Sym::Eof, 18)]);
    }

    #[test]
    fn tables_fail() {
        fail!("[.]", Error::Unexpected { pos: 1 });
        fail!("[hello", Error::Unexpected { pos: 6 });
        fail!("[hello.]", Error::Unexpected { pos: 7 });
        fail!("[.world]", Error::Unexpected { pos: 1 });
        fail!("[hello.\nworld]", Error::MultilineKey { pos: 7 });
        fail!("[hello.'''world''']", Error::MultilineString { pos: 7 });
        fail!(r#"[hello."""world"""]"#, Error::MultilineString { pos: 7 });
        fail!("[[.]]", Error::Unexpected { pos: 2 });
        fail!("[[hello", Error::Unexpected { pos: 7 });
        fail!("[[hello]", Error::Expected { pos: 8, c: ']' });
        fail!("[[hello.]]", Error::Unexpected { pos: 8 });
        fail!("[[.world]]", Error::Unexpected { pos: 2 });
        fail!("[[hello.\nworld]]", Error::MultilineKey { pos: 8 });
        fail!("[[hello.'''world''']]", Error::MultilineString { pos: 8 });
        fail!(
            r#"[[hello."""world"""]]"#,
            Error::MultilineString { pos: 8 }
        );
    }

    #[test]
    fn tables_success() {
        succ!(
            "[test-1]",
            &[
                Symbol::new(Sym::Table, 1),
                Symbol::with_span(Sym::Key, 1, 6),
                Symbol::new(Sym::TableEnd, 8),
                Symbol::new(Sym::Eof, 8)
            ]
        );
        succ!(
            "[hello.world]",
            &[
                Symbol::new(Sym::Table, 1),
                Symbol::with_span(Sym::Key, 1, 5),
                Symbol::with_span(Sym::Key, 7, 11),
                Symbol::new(Sym::TableEnd, 13),
                Symbol::new(Sym::Eof, 13)
            ]
        );
        succ!(
            r#"[hello."zing bing bang"]"#,
            &[
                Symbol::new(Sym::Table, 1),
                Symbol::with_span(Sym::Key, 1, 5),
                Symbol::with_span(Sym::String, 8, 22),
                Symbol::new(Sym::TableEnd, 24),
                Symbol::new(Sym::Eof, 24),
            ]
        );
        succ!(
            r#"[ j . "????" . 'l' ]"#,
            &[
                Symbol::new(Sym::Table, 1),
                Symbol::with_span(Sym::Key, 2, 2),
                Symbol::with_span(Sym::String, 7, 11),
                Symbol::with_span(Sym::String, 16, 16),
                Symbol::new(Sym::TableEnd, 20),
                Symbol::new(Sym::Eof, 20),
            ]
        );
    }

    #[test]
    fn arrays_fail() {
        fail!("a = [,]", Error::Unexpected { pos: 5 });
        fail!("a = [true false]", Error::MissingDelimiter { pos: 10 });
    }

    #[test]
    fn arrays_success() {
        succ!(
            "a = []",
            &[
                Symbol::new(Sym::Key, 0),
                Symbol::new(Sym::Assign, 2),
                Symbol::new(Sym::Array, 4),
                Symbol::new(Sym::ArrayEnd, 5),
                Symbol::new(Sym::Eof, 6),
            ]
        );

        succ!(
            "a = [
                true      ,
                false # false is false
            ]",
            &[
                Symbol::new(Sym::Key, 0),
                Symbol::new(Sym::Assign, 2),
                Symbol::new(Sym::Array, 4),
                Symbol::with_span(Sym::Bool, 22, 26),
                Symbol::with_span(Sym::Bool, 50, 55),
                Symbol::new(Sym::ArrayEnd, 85),
                Symbol::new(Sym::Eof, 86),
            ]
        );

        succ!(
            r#"colors = [ "red", "yellow", [ "green", "purple", ], ]"#,
            &[
                Symbol::with_span(Sym::Key, 0, 5),
                Symbol::new(Sym::Assign, 7),
                Symbol::new(Sym::Array, 9),
                Symbol::with_span(Sym::String, 12, 15),
                Symbol::with_span(Sym::String, 19, 25),
                Symbol::new(Sym::Array, 28),
                Symbol::with_span(Sym::String, 31, 36),
                Symbol::with_span(Sym::String, 40, 46),
                Symbol::new(Sym::ArrayEnd, 49),
                Symbol::new(Sym::ArrayEnd, 52),
                Symbol::new(Sym::Eof, 53),
            ]
        );
    }

    #[test]
    fn inline_tables_fail() {
        fail!("colors = { red = true, }", Error::Unexpected { pos: 23 });
        fail!(
            "colors = { red
            = true }",
            Error::Unexpected { pos: 14 }
        );
    }

    #[test]
    fn inline_tables_success() {
        succ!(
            "colors = { red = true }",
            &[
                Symbol::with_span(Sym::Key, 0, 5),
                Symbol::new(Sym::Assign, 7),
                Symbol::new(Sym::InlineTable, 9),
                Symbol::with_span(Sym::Key, 11, 13),
                Symbol::new(Sym::Assign, 15),
                Symbol::with_span(Sym::Bool, 17, 21),
                Symbol::new(Sym::InlineTableEnd, 22),
                Symbol::new(Sym::Eof, 23)
            ]
        );
        succ!(
            r#"colors = { red = true, green = '0x00ff00' }"#,
            &[
                Symbol::with_span(Sym::Key, 0, 5),
                Symbol::new(Sym::Assign, 7),
                Symbol::new(Sym::InlineTable, 9),
                Symbol::with_span(Sym::Key, 11, 13),
                Symbol::new(Sym::Assign, 15),
                Symbol::with_span(Sym::Bool, 17, 21),
                Symbol::with_span(Sym::Key, 23, 27),
                Symbol::new(Sym::Assign, 29),
                Symbol::with_span(Sym::String, 32, 39),
                Symbol::new(Sym::InlineTableEnd, 42),
                Symbol::new(Sym::Eof, 43)
            ]
        );
        succ!(
            r#"animal = { type.name = "pug" }"#,
            &[
                Symbol::with_span(Sym::Key, 0, 5),
                Symbol::new(Sym::Assign, 7),
                Symbol::new(Sym::InlineTable, 9),
                Symbol::with_span(Sym::Key, 11, 14),
                Symbol::with_span(Sym::Key, 16, 19),
                Symbol::new(Sym::Assign, 21),
                Symbol::with_span(Sym::String, 24, 27),
                Symbol::new(Sym::InlineTableEnd, 29),
                Symbol::new(Sym::Eof, 30)
            ]
        );
        succ!(
            r#"points = [ { x = +1, y = +2, z = +3 },
           { x = +7, y = +8, z = +9 },
           { x = +2, y = +4, z = +8 } ]"#,
            &[
                Symbol::with_span(Sym::Key, 0, 5),
                Symbol::new(Sym::Assign, 7),
                Symbol::new(Sym::Array, 9),
                Symbol::new(Sym::InlineTable, 11),
                Symbol::with_span(Sym::Key, 13, 13),
                Symbol::new(Sym::Assign, 15),
                Symbol::with_span(Sym::Integer, 17, 18),
                Symbol::with_span(Sym::Key, 21, 21),
                Symbol::new(Sym::Assign, 23),
                Symbol::with_span(Sym::Integer, 25, 26),
                Symbol::with_span(Sym::Key, 29, 29),
                Symbol::new(Sym::Assign, 31),
                Symbol::with_span(Sym::Integer, 33, 34),
                Symbol::new(Sym::InlineTableEnd, 36),
                Symbol::new(Sym::InlineTable, 50),
                Symbol::with_span(Sym::Key, 52, 52),
                Symbol::new(Sym::Assign, 54),
                Symbol::with_span(Sym::Integer, 56, 57),
                Symbol::with_span(Sym::Key, 60, 60),
                Symbol::new(Sym::Assign, 62),
                Symbol::with_span(Sym::Integer, 64, 65),
                Symbol::with_span(Sym::Key, 68, 68),
                Symbol::new(Sym::Assign, 70),
                Symbol::with_span(Sym::Integer, 72, 73),
                Symbol::with_span(Sym::InlineTableEnd, 75, 75),
                Symbol::with_span(Sym::InlineTable, 89, 89),
                Symbol::with_span(Sym::Key, 91, 91),
                Symbol::new(Sym::Assign, 93),
                Symbol::with_span(Sym::Integer, 95, 96),
                Symbol::with_span(Sym::Key, 99, 99),
                Symbol::new(Sym::Assign, 101),
                Symbol::with_span(Sym::Integer, 103, 104),
                Symbol::with_span(Sym::Key, 107, 107),
                Symbol::new(Sym::Assign, 109),
                Symbol::with_span(Sym::Integer, 111, 112),
                Symbol::with_span(Sym::InlineTableEnd, 114, 114),
                Symbol::new(Sym::ArrayEnd, 116),
                Symbol::new(Sym::Eof, 117),
            ]
        );
    }
}
