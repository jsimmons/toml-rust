use memchr::memmem;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Encounted an illegal control character in a comment.
    IllegalControlCharacter {
        pos: BytePos,
    },
    /// A string has too many sequential quote characters.
    TooManyQuotesInString {
        start: BytePos,
        pos: BytePos,
    },
    /// String was not terminated.
    UnterminatedString {
        start: BytePos,
        pos: BytePos,
    },
    /// A newline was encountered while parsing a key.
    MultilineKey {
        pos: BytePos,
    },
    /// The parser did not consume the entire input string.
    UnconsumedInput {
        pos: BytePos,
    },
    Unexpected {
        pos: BytePos,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BytePos(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    lo: BytePos,
    hi: BytePos,
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
    fn peek(&self) -> u8 {
        *self.text.as_bytes().get(self.index + 1).unwrap_or(&0)
    }

    #[inline(always)]
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == c {
            self.next();
            true
        } else {
            false
        }
    }

    #[inline(always)]
    fn advance_to(&mut self, index: usize) {
        self.index = index;
    }

    #[inline(always)]
    fn next(&mut self) {
        self.index += 1;
        self.current = *self.text.as_bytes().get(self.index).unwrap_or(&0);
    }

    #[inline(always)]
    fn push(&mut self, sym: Sym) {
        self.symbols.push(Symbol {
            sym,
            span: Span {
                lo: BytePos(self.index),
                hi: BytePos(self.index),
            },
        })
    }

    #[inline(always)]
    fn push_span(&mut self, sym: Sym, lo: usize, hi: usize) {
        self.symbols.push(Symbol {
            sym,
            span: Span {
                lo: BytePos(lo),
                hi: BytePos(hi),
            },
        })
    }

    #[cold]
    fn err_unterminated_string(&self, start: usize) -> Result<(), Error> {
        Err(Error::UnterminatedString {
            start: BytePos(start),
            pos: BytePos(self.index),
        })
    }

    #[cold]
    fn err_too_many_quotes_in_string(&self, start: usize) -> Result<(), Error> {
        Err(Error::TooManyQuotesInString {
            start: BytePos(start),
            pos: BytePos(self.index),
        })
    }

    #[cold]
    fn err_illegal_control_character(&self) -> Result<(), Error> {
        Err(Error::IllegalControlCharacter {
            pos: BytePos(self.index),
        })
    }

    #[cold]
    fn err_multiline_key(&self) -> Result<(), Error> {
        Err(Error::MultilineKey {
            pos: BytePos(self.index),
        })
    }

    #[cold]
    fn err_unconsumed_input(&self) -> Result<(), Error> {
        Err(Error::UnconsumedInput {
            pos: BytePos(self.index),
        })
    }

    #[cold]
    fn err_unexpected(&self) -> Result<(), Error> {
        Err(Error::Unexpected {
            pos: BytePos(self.index),
        })
    }

    fn scan_key(&mut self) -> Result<(), Error> {
        let start = self.index;
        loop {
            self.next();
            match self.current {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {}
                b' ' | b'\t' | b'=' | b'.' | b']' => {
                    self.push_span(Sym::Key, start, self.index - 1);
                    return Ok(());
                }
                _ => self.err_unexpected()?,
            }
        }
    }

    fn scan_string(&mut self) -> Result<(), Error> {
        let text = self.text.as_bytes();
        match &text[self.index..] {
            [b'\'', b'\'', b'\'', rest @ ..] => {
                let start = self.index + 2;
                if let Some(index) = memmem::find(rest, &[b'\'', b'\'', b'\'']) {
                    self.advance_to(start + index + 3);
                    if self.eat(b'\'') {
                        if self.eat(b'\'') {
                            if self.peek() == b'\'' {
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
            [b'\'', rest @ ..] => {
                let start = self.index + 1;
                if let Some(index) = memchr::memchr2(b'\n', b'\'', rest) {
                    self.advance_to(start + index + 1);
                    if text[index] == b'\'' {
                        self.push_span(Sym::String, start, index);
                        return Ok(());
                    }
                }
                self.err_unterminated_string(start)
            }
            [b'"', b'"', b'"', ..] => {
                let start = self.index + 2;
                self.advance_to(start);
                let mut quote_count = 0;
                let mut slash_count = 0;
                loop {
                    self.next();
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
                }
            }
            [b'"', ..] => {
                let start = self.index + 1;
                self.advance_to(start);
                let mut slash_count = 0;
                loop {
                    self.next();
                    match self.current {
                        b'"' => {
                            if slash_count & 1 == 0 {
                                self.push_span(Sym::String, start, self.index);
                                return Ok(());
                            }
                            slash_count = 0;
                        }
                        b'\\' => {
                            slash_count += 1;
                        }
                        b'\n' | 0 => {
                            self.err_unterminated_string(start)?;
                        }
                        _ => {
                            slash_count = 0;
                        }
                    }
                }
            }
            _ => panic!(),
        }
    }

    fn scan_value(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn scan_table(&mut self) -> Result<(), Error> {
        self.next();
        let mut saw_dot = true;
        loop {
            match self.current {
                b'\r' => {
                    if !self.eat(b'\n') {
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
                    self.scan_string()?
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    if !saw_dot {
                        self.err_unexpected()?
                    }
                    self.scan_key()?;
                    println!("{}", self.current as char);
                    match self.current {
                        b'.' => {
                            saw_dot = true;
                            self.next();
                        }
                        b']' => {
                            self.next();
                            break;
                        }
                        _ => self.err_unexpected()?,
                    }
                }
                _ => self.err_unexpected()?,
            }
        }

        Ok(())
    }

    fn scan_dotted(&mut self) -> Result<(), Error> {
        self.next();
        let mut saw_dot = false;
        loop {
            match self.current {
                b'\r' => {
                    if !self.eat(b'\n') {
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
                    self.push(Sym::Assign);
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
                    self.scan_key()?;
                    saw_dot = self.current == b'.';
                    self.next();
                }
                _ => self.err_unexpected()?,
            }
        }

        Ok(())
    }

    pub fn scan(&mut self) -> Result<(), Error> {
        'outer: loop {
            match self.current {
                b'\r' => {
                    self.next();
                    if !self.eat(b'\n') {
                        self.err_illegal_control_character()?;
                    }
                }
                b'\n' | b' ' | b'\t' => self.next(),
                b'#' => 'comment: loop {
                    self.next();
                    match self.current {
                        b'\n' => break 'comment,
                        b'\r' => {
                            if self.eat(b'\n') {
                                break 'comment;
                            }
                            self.err_illegal_control_character()?;
                        }
                        0x1..=0x8 | 0xa..=0x1f | 0x7f => {
                            self.err_illegal_control_character()?;
                        }
                        0x0 => {
                            break 'outer;
                        }
                        _ => {}
                    }
                },
                b'[' => {
                    let sym = if self.eat(b'[') {
                        Sym::ArrayOfTable
                    } else {
                        Sym::Table
                    };
                    self.push(sym);
                    self.scan_table()?;
                    if sym == Sym::ArrayOfTable {
                        if !self.eat(b']') {
                            self.err_unexpected()?;
                        }
                    }
                    self.push(Sym::TableEnd);
                }
                b'"' | b'\'' => {
                    self.scan_string()?;
                    self.scan_dotted()?;
                    self.scan_value()?;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    self.scan_key()?;
                    self.scan_dotted()?;
                    self.scan_value()?;
                }
                0 => break 'outer,
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

    fn fail(text: &str, err: Error) {
        let mut lex = Lex::new(text);
        assert_eq!(lex.scan(), Err(err));
    }

    fn success(text: &str, expected_symbols: &[Symbol]) {
        let mut lex = Lex::new(text);
        lex.scan().expect("parse failed");
        let found_symbols = &lex.symbols[..];
        let max_len = expected_symbols.len().max(found_symbols.len());
        for i in 0..max_len {
            let (expected, found) = (expected_symbols.get(i), found_symbols.get(i));
            if expected != found {
                for j in 0..max_len {
                    let (expected, found) = (expected_symbols.get(j), found_symbols.get(j));
                    eprintln!(
                        "{} {:<4} === {}",
                        if i == j { "==>" } else { "   " },
                        format!("{:?}", expected),
                        format!("{:?}", found),
                    );
                }
                panic!("expected {expected:?} but found {found:?}")
            }
        }
    }

    #[test]
    fn basic_failures() {
        fail(r#"="#, Error::Unexpected { pos: BytePos(0) });
    }

    #[test]
    fn basic_success() {
        success(
            "",
            &[Symbol {
                sym: Sym::Eof,
                span: Span {
                    lo: BytePos(0),
                    hi: BytePos(0),
                },
            }],
        );
        success(
            "# Hello,\n# World!",
            &[Symbol {
                sym: Sym::Eof,
                span: Span {
                    lo: BytePos(17),
                    hi: BytePos(17),
                },
            }],
        );
        success(
            "# Hello,\r\n# World!",
            &[Symbol {
                sym: Sym::Eof,
                span: Span {
                    lo: BytePos(18),
                    hi: BytePos(18),
                },
            }],
        );
    }

    #[test]
    fn tables_fail() {
        fail("[.]", Error::Unexpected { pos: BytePos(1) });
        fail("[hello.]", Error::Unexpected { pos: BytePos(7) });
    }

    #[test]
    fn tables_success() {
        success(
            "[hello.world]",
            &[Symbol {
                sym: Sym::Eof,
                span: Span {
                    lo: BytePos(18),
                    hi: BytePos(18),
                },
            }],
        );
    }
}
