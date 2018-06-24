extern crate byteorder;

use std::str;
use std::io::Error;
use std::io::Read;
use std::result::Result;
use self::byteorder::{ByteOrder, BigEndian, WriteBytesExt};
use num_traits::{FromPrimitive};

#[derive(Debug, PartialEq, Primitive)]
#[repr(u8)]
pub enum FeslMessageType {
    SingleClient = 0xc0,
    SingleServer = 0x80,
    MultiClient = 0xf0,
    MultiServer = 0xb0
}

#[derive(Debug)]
pub enum FeslMessageError {
    ExpectedDelimiter,
    ExpectedUtf8(str::Utf8Error),
    InvalidCommandLength,
    InvalidType
}

impl From<str::Utf8Error> for FeslMessageError {
    fn from(error: str::Utf8Error) -> Self {
        FeslMessageError::ExpectedUtf8(error)
    }
}

type FeslMessageResult<T> = Result<T, FeslMessageError>;

#[derive(Debug)]
pub struct FeslMessage {
    data: Box<[u8]>
}

impl FeslMessage {
    // TODO: implement more sources in single `from(src)` method signature
    // TODO: len should be bounded for overflow, etc
    pub fn from_read<T: Read>(src: &mut T) -> Result<FeslMessage, Error> {
        let mut header = [0u8; 12];
        src.read_exact(&mut header)?;
        let len = BigEndian::read_u32(&header[8..12]) as usize;
        let mut buf: Vec<u8> = Vec::with_capacity(len);
        buf.extend(&header);
        src.take((len - 12) as u64).read_to_end(&mut buf)?;
        Ok(FeslMessage {
            data: buf.into_boxed_slice()
        })
    }

    pub fn get_cmd(&self) -> Result<&str, str::Utf8Error> {
        Ok(str::from_utf8(&self.data[0..4])?)
    }

    pub fn get_type(&self) -> FeslMessageResult<FeslMessageType> {
        let val = self.data[4] & 0xf0;
        FeslMessageType::from_u8(val).ok_or_else(|| FeslMessageError::InvalidType)
    }

    pub fn get_id(&self) -> u32 {
        BigEndian::read_u32(&self.data[4..12]) & 0xfffffff
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..]
    }
}

#[derive(Debug)]
pub struct FeslMessageIterator<'a> {
    src: &'a [u8]
}

impl <'a> FeslMessageIterator<'a> {
    fn index_of(&self, token: u8) -> Option<usize> {
        self.src.iter().position(|&x| x == token)
    }

    fn shift_slice(&mut self, token: u8) -> FeslMessageResult<&'a [u8]> {
        let x = match self.index_of(token) {
            Some(x) => x,
            _ => return Err(FeslMessageError::ExpectedDelimiter)
        };
        let val = &self.src[..x];
        self.src = &self.src[x + 1..];
        Ok(val)
    }

    fn shift_str(&mut self, token: u8) -> FeslMessageResult<&'a str> {
        Ok(str::from_utf8(self.shift_slice(token)?)?)
    }

    fn read(&mut self) -> FeslMessageResult<(&'a str, &'a str)> {
        Ok((&self.shift_str(b'=')?, &self.shift_str(b'\n')?))
    }

    fn end<T, E>(&mut self, val: E) -> Result<T, E> {
        self.src = &self.src[self.src.len()..];
        Err(val)
    }
}

impl <'a> Iterator for FeslMessageIterator<'a> {
    type Item = Result<(&'a str, &'a str), FeslMessageError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.src.len() {
            0 => None,
            1 => match self.shift_slice(0x00).or_else(|x| self.end(x)) {
                Err(v) => Some(Err(v)),
                _ => None
            },
            _ => Some(self.read().or_else(|x| self.end(x)))
        }
    }
}

impl <'a> IntoIterator for &'a FeslMessage {
    type Item = FeslMessageResult<(&'a str, &'a str)>;
    type IntoIter = FeslMessageIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FeslMessageIterator {
            src: &self.data[12..]
        }
    }
}

type FeslMessageBuilderResult<'a> = FeslMessageResult<FeslMessageBuilder<'a>>;

#[derive(Debug)]
pub struct FeslMessageBuilder<'a> {
    cmd: &'a str,
    type_and_id: u32,
    len: usize,
    buf: Vec<(&'a str, &'a str)>
}

impl <'a> FeslMessageBuilder<'a> {
    pub fn new(cmd: &'a str, fesl_type: FeslMessageType, id: u32) -> FeslMessageBuilderResult<'a> {
        if cmd.len() != 4 {
            return Err(FeslMessageError::InvalidCommandLength);
        }
        Ok(FeslMessageBuilder {
            cmd,
            type_and_id: (((fesl_type as u32) & 0xf0) << 24) | (id & 0xfffffff),
            len: 13,
            buf: Vec::new()
        })
    }

    pub fn push(&mut self, key: &'a str, value: &'a str) {
        self.len += key.len() + 1 + value.len() + 1;
        self.buf.push((key, value))
    }

    pub fn build(self) -> FeslMessage {
        let mut buf: Vec<u8> = Vec::with_capacity(self.len);
        buf.extend(self.cmd.as_bytes());
        buf.write_u32::<BigEndian>(self.type_and_id).unwrap();
        buf.write_u32::<BigEndian>(self.len as u32).unwrap();
        for (key, value) in &self.buf {
            buf.extend(key.as_bytes());
            buf.push(b'=');
            buf.extend(value.as_bytes());
            buf.push(b'\n');
        }
        buf.push(0x00);
        let data = buf.into_boxed_slice();
        FeslMessage {data}
    }
}
