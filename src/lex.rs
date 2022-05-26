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
            span: Span { lo: pos, hi: pos },
        }
    }

    pub const fn with_span(sym: Sym, lo: usize, hi: usize) -> Self {
        Symbol {
            sym,
            span: Span { lo, hi },
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
    symbols: Vec<Symbol>,
}

impl<'a> Lex<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            index: 0,
            current: *text.as_bytes().get(0).unwrap_or(&0),
            symbols: Vec::new(),
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
        let symbol = Symbol::new(sym, self.index);
        //println!("{:?}", symbol);
        self.symbols.push(symbol)
    }

    #[inline(always)]
    fn push_span(&mut self, sym: Sym, lo: usize, hi: usize) {
        let symbol = Symbol::with_span(sym, lo, hi);
        //println!("{:?}", symbol);
        self.symbols.push(symbol)
    }

    #[cold]
    fn err_unterminated_string(&self, start: usize) -> Result<(), Error> {
        Err(Error::UnterminatedString {
            start,
            pos: self.index,
        })
    }

    #[cold]
    fn err_too_many_quotes_in_string(&self, start: usize) -> Result<(), Error> {
        Err(Error::TooManyQuotesInString {
            start,
            pos: self.index,
        })
    }

    #[cold]
    fn err_illegal_control_character(&self) -> Result<(), Error> {
        Err(Error::ControlCharacter { pos: self.index })
    }

    #[cold]
    fn err_multiline_key(&self) -> Result<(), Error> {
        Err(Error::MultilineKey { pos: self.index })
    }

    #[cold]
    fn err_illegal_multiline_string(&self) -> Result<(), Error> {
        Err(Error::MultilineString { pos: self.index })
    }

    #[cold]
    fn err_unconsumed_input(&self) -> Result<(), Error> {
        Err(Error::UnconsumedInput { pos: self.index })
    }

    #[cold]
    fn err_expected(&self, c: u8) -> Result<(), Error> {
        Err(Error::Expected {
            pos: self.index,
            c: c as char,
        })
    }

    #[cold]
    fn err_unexpected(&self) -> Result<(), Error> {
        Err(Error::Unexpected { pos: self.index })
    }

    fn consume_comment(&mut self) -> Result<(), Error> {
        loop {
            self.next();
            match self.current {
                b'\n' => break,
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.next();
                    self.next();
                    break;
                }
                0x1..=0x8 | 0xa..=0x1f | 0x7f => {
                    self.err_illegal_control_character()?;
                }
                0x0 => {
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn consume_line(&mut self) -> Result<(), Error> {
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
                b' ' | b'\t' => self.next(),
                b'\n' | 0 => break,
                b'#' => {
                    self.consume_comment()?;
                    break;
                }
                _ => self.err_unexpected()?,
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
                            self.push_span(Sym::String, start, self.index - extra);
                            return Ok(());
                        }
                        _ => return self.err_too_many_quotes_in_string(start),
                    }

                    slash_count += 1;
                    quote_count = 0;
                }
                ch @ _ => {
                    match quote_count {
                        0 | 1 | 2 => {
                            if ch == 0 {
                                return self.err_unterminated_string(start);
                            }
                        }
                        3 | 4 | 5 => {
                            let extra = quote_count - 3;
                            self.push_span(Sym::String, start, self.index - extra);
                            return Ok(());
                        }
                        _ => self.err_too_many_quotes_in_string(start)?,
                    }
                    quote_count = 0;
                }
            }

            self.next();
        }
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

    fn scan_array(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn scan_inline_table(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn scan_value(&mut self) -> Result<(), Error> {
        loop {
            match self.current {
                b'\r' => {
                    if self.peek() != b'\n' {
                        self.err_illegal_control_character()?;
                    }
                    self.err_unexpected()?
                }
                b'\n' => self.err_unexpected()?,
                b' ' | b'\t' => self.next(),
                b'"' | b'\'' => break self.scan_string(),
                b'{' => break self.scan_inline_table(),
                b'[' => break self.scan_array(),
                _ => self.err_unexpected()?,
            }
        }
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

    fn scan_dotted(&mut self) -> Result<(), Error> {
        self.next();
        let mut saw_dot = false;
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
                    self.scan_key()?
                }
                _ => self.err_unexpected()?,
            }
        }

        self.push(Sym::Assign);
        self.next();

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
                b'"' | b'\'' => {
                    self.scan_string()?;
                    self.scan_dotted()?;
                    self.scan_value()?;
                    self.consume_line()?;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    self.scan_key()?;
                    self.scan_dotted()?;
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
    }
}
