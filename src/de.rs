use crate::{Error, Result};
use serde::{de, Deserialize};

// Should be enough for anybody.
const SCRATCH_LEN: usize = 64 * 1024;

// Keep it secret, keep it safe.
mod private {
    pub trait Sealed {}
}

pub trait Read<'de>: private::Sealed {
    fn current(&self) -> usize;
    fn eof(&self) -> bool;
    fn peek(&self) -> u8;
    fn peek2(&self) -> u8;
    fn next(&mut self) -> u8;
    fn refill(&mut self);
}

pub struct StrRead<'de> {
    source: &'de str,
    current: usize,
}

impl<'de> StrRead<'de> {
    pub fn new(source: &'de str) -> Self {
        Self { source, current: 0 }
    }
}

impl<'de> private::Sealed for StrRead<'de> {}

impl<'de> Read<'de> for StrRead<'de> {
    #[inline(always)]
    fn current(&self) -> usize {
        self.current
    }

    #[inline(always)]
    fn eof(&self) -> bool {
        self.current == self.source.len()
    }

    #[inline(always)]
    fn peek(&self) -> u8 {
        *self.source.as_bytes().get(self.current).unwrap_or(&0)
    }

    #[inline(always)]
    fn peek2(&self) -> u8 {
        *self.source.as_bytes().get(self.current + 1).unwrap_or(&0)
    }

    #[inline(always)]
    fn next(&mut self) -> u8 {
        match self.source.as_bytes().get(self.current) {
            Some(&c) => {
                self.current += 1;
                c
            }
            None => 0,
        }
    }

    #[cold]
    fn refill(&mut self) {
        todo!()
    }
}

struct IoRead<R>
where
    R: std::io::Read,
{
    reader: R,
    base: usize,
    current: usize,
    scratch: Box<[u8; SCRATCH_LEN]>,
}

impl<R> IoRead<R>
where
    R: std::io::Read,
{
    fn new(reader: R) -> Self {
        Self {
            reader,
            base: 0,
            current: 0,
            scratch: Box::new([0; SCRATCH_LEN]),
        }
    }
}

impl<R> private::Sealed for IoRead<R> where R: std::io::Read {}

impl<'de, R> Read<'de> for IoRead<R>
where
    R: std::io::Read,
{
    #[inline(always)]
    fn current(&self) -> usize {
        todo!()
    }

    fn eof(&self) -> bool {
        todo!()
    }

    #[inline(always)]
    fn peek(&self) -> u8 {
        todo!()
    }

    #[inline(always)]
    fn peek2(&self) -> u8 {
        todo!()
    }

    #[inline(always)]
    fn next(&mut self) -> u8 {
        todo!()
    }

    #[cold]
    fn refill(&mut self) {
        todo!()
    }
}

enum Token {
    LiteralString,
    BasicString,
}

pub struct Lex<R> {
    read: R,
    current_line: usize,
}

impl<'de, R> Lex<R>
where
    R: Read<'de>,
{
    pub fn new(read: R) -> Self {
        Self {
            read,
            current_line: 0,
        }
    }

    pub fn eof(&self) -> bool {
        self.read.eof()
    }

    fn lex_header(&mut self) {
        self.read.next();
        let is_array = if self.read.peek() == b'[' {
            self.read.next();
            true
        } else {
            false
        };
        let start = self.read.current();

        loop {
            match self.read.next() {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {}
                b'\'' => {}
                b'"' => {}
                b']' => {
                    if is_array {
                        if self.read.next() == b']' {
                            break;
                        } else {
                            todo!()
                        }
                    } else {
                        break;
                    }
                }
                _ => {}
            }
        }

        let finish = self.read.current();
        println!("header (array: {}): [{}..{}]", is_array, start, finish);
    }

    fn lex_key(&mut self) {
        let start = self.read.current();
        loop {
            match self.read.next() {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {}
                _ => break,
            }
        }
        let end = self.read.current();
        println!("key: [{},{}]", start, end);
    }

    fn lex_literal_string(&mut self) {
        let start = self.read.current();

        loop {
            match self.read.next() {
                b'"' => {
                    break;
                }
                _ => {}
            }
        }

        let end = self.read.current();

        println!("string: [{}, {}]", start, end);
    }

    fn lex_multiline_literal_string(&mut self) {
        let start = self.read.current();
        println!("string: {}", start);
    }

    fn lex_basic_string(&mut self) {
        let start = self.read.current();

        loop {
            match self.read.next() {
                b'\\' => match self.read.peek() {
                    b'b' | b't' | b'n' | b'f' | b'r' | b'"' | b'\\' => {
                        self.read.next();
                    }
                    b'u' => {
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                    }
                    b'U' => {
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                        self.read.next();
                    }
                    _ => todo!(),
                },
                b'"' => {
                    break;
                }
                _ => {}
            }
        }

        let end = self.read.current();

        println!("ya basic: [{}, {}]", start, end);
    }

    fn lex_multiline_basic_string(&mut self) {
        let start = self.read.current();
        println!("ya basic: {}", start);
    }

    pub fn lex(&mut self) {
        loop {
            match self.read.peek() {
                b' ' | b'\t' => {
                    self.read.next();
                }
                b'\n' => {
                    self.read.next();
                    self.current_line += 1
                }
                b'\r' => {
                    self.read.next();
                    if self.read.peek() == b'\n' {
                        self.read.next();
                        self.current_line += 1;
                    } else {
                        todo!() // error: unaccompanied carrage return. ill-eagle
                    }
                }
                b'#' => loop {
                    self.read.next();
                    match self.read.next() {
                        b'\n' => {
                            self.current_line += 1;
                            break;
                        }
                        b'\r' => {
                            if self.read.peek() == b'\n' {
                                self.read.next();
                                self.current_line += 1;
                                break;
                            }
                            todo!() // error: unaccompanied carrage return. ill-eagle
                        }
                        0x0..=0x8 | 0xa..=0x1f | 0x7f => {
                            todo!() // error: ill-eagle control character in comment.
                        }
                        _ => {}
                    }
                },
                b'=' => {
                    self.read.next();
                    println!("eq {}", self.read.current())
                }
                b'[' => {
                    self.lex_header();
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => {
                    self.lex_key();
                }
                b'\'' => {
                    self.read.next();
                    if self.read.peek() == b'\'' && self.read.peek2() == b'\'' {
                        self.read.next();
                        self.read.next();
                        self.lex_multiline_literal_string();
                    } else {
                        self.lex_literal_string();
                    };
                }
                b'"' => {
                    self.read.next();
                    if self.read.peek() == b'\"' && self.read.peek2() == b'\"' {
                        self.read.next();
                        self.read.next();
                        self.lex_multiline_basic_string();
                    } else {
                        self.lex_basic_string();
                    };
                }
                0 => {
                    println!("eof: {}", self.read.current());
                    break;
                }
                _ => {
                    println!("error: {}", self.read.current());
                    break;
                }
            }
        }
    }
}

pub struct Deserializer<R> {
    lex: Lex<R>,
}

impl<'de, R> Deserializer<R>
where
    R: Read<'de>,
{
    fn new(lex: Lex<R>) -> Self {
        Self { lex }
    }

    fn end(&self) -> Result<()> {
        Ok(())
    }
}

// impl<'de, 'a, R: Read<'de>> de::Deserializer<'de> for &'a mut Deserializer<R> {
//     type Error = Error;

//     fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_unit_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_newtype_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_tuple_struct<V>(
//         self,
//         name: &'static str,
//         len: usize,
//         visitor: V,
//     ) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_struct<V>(
//         self,
//         name: &'static str,
//         fields: &'static [&'static str],
//         visitor: V,
//     ) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_enum<V>(
//         self,
//         name: &'static str,
//         variants: &'static [&'static str],
//         visitor: V,
//     ) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }

//     fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
//     where
//         V: de::Visitor<'de>,
//     {
//         todo!()
//     }
// }

// fn from_trait<'de, R, T>(read: R) -> Result<T>
// where
//     R: Read<'de>,
//     T: de::Deserialize<'de>,
// {
//     let mut de = Deserializer::new(Lex::new(read));
//     let value = de::Deserialize::deserialize(&mut de)?;
//     de.end()?;
//     Ok(value)
// }

pub fn from_str<'a, T>(source: &'a str) -> Result<T>
where
    T: Deserialize<'a>,
{
    from_trait(StrRead::new(source))
}

pub fn from_reader<R, T>(reader: R) -> Result<T>
where
    R: std::io::Read,
    T: de::DeserializeOwned,
{
    from_trait(IoRead::new(reader))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_whitespace() {
        let mut lex = Lex::new(StrRead::new("\t  \t"));
        lex.lex();
        assert!(lex.eof());

        let mut lex = Lex::new(StrRead::new("\n \n\n \n"));
        lex.lex();
        assert!(lex.eof());

        let mut lex = Lex::new(StrRead::new("\r\n \r\n\r\n "));
        lex.lex();
        assert!(lex.eof());
    }
}
