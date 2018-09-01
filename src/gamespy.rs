use std::str;
use std::io;
use std::io::{Write, BufRead, BufReader};
use std::net::TcpStream;
use std::result::Result;

#[derive(Debug)]
pub enum GameSpyPacketError {
    ExpectedDelimiter,
    ExpectedUtf8(str::Utf8Error)
}

impl From<str::Utf8Error> for GameSpyPacketError {
    fn from(error: str::Utf8Error) -> Self {
        GameSpyPacketError::ExpectedUtf8(error)
    }
}

type GameSpyPacketResult<T> = Result<T, GameSpyPacketError>;

#[derive(Debug)]
pub struct GameSpyPacket {
    data: Box<[u8]>
}

impl GameSpyPacket {
    pub fn from_box(data: Box<[u8]>) -> GameSpyPacket {
        GameSpyPacket {
            data
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..]
    }
}

pub struct GameSpyPacketConsumer<'a> {
    src: &'a TcpStream,
    reader: BufReader<&'a TcpStream>
}

impl <'a> GameSpyPacketConsumer<'a> {
    pub fn new(src: &'a TcpStream) -> GameSpyPacketConsumer<'a> {
        GameSpyPacketConsumer {
            src,
            reader: BufReader::new(&src)
        }
    }
}

impl <'a> Write for GameSpyPacketConsumer<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { self.src.write(&buf) }
    fn flush(&mut self) -> io::Result<()> { self.src.flush() }
}

impl <'a> Iterator for GameSpyPacketConsumer<'a> {
    type Item = GameSpyPacket;

    fn next(&mut self) -> Option<GameSpyPacket> {
        const TERM: &[u8; 7] = b"\\final\\";
        let mut msg: Vec<u8> = Vec::new();
        let mut rank = 0;
        loop {
            let read = {
                let mut buf = self.reader.fill_buf().unwrap();
                let mut i = 0;
                for &item in buf {
                    i += 1;
                    if item == TERM[rank] {
                        rank += 1;
                        if rank >= 7 {
                            break;
                        }
                    } else if item == TERM[0] {
                        rank = 1;
                    } else {
                        rank = 0;
                    }
                }
                msg.extend(&buf[..i]);
                i
            };
            self.reader.consume(read);
            if rank >= 7 {
                return Some(GameSpyPacket {
                    data: msg.into_boxed_slice()
                });
            }
        }
    }
}

#[derive(Debug)]
pub struct GameSpyPacketIterator<'a> {
    src: &'a [u8]
}

impl <'a> GameSpyPacketIterator<'a> {
    fn shift_slice(&mut self, token: u8) -> GameSpyPacketResult<&'a [u8]> {
        if self.src[0] != b'\\' {
            return Err(GameSpyPacketError::ExpectedDelimiter);
        }
        let x = match self.src[1..].iter().position(|&x| x == token) {
            Some(x) => x + 1,
            _ => self.src.len()
        };
        let val = &self.src[1..x];
        self.src = &self.src[x..];
        Ok(val)
    }

    fn shift_str(&mut self, token: u8) -> GameSpyPacketResult<&'a str> {
        Ok(str::from_utf8(self.shift_slice(token)?)?)
    }

    fn read(&mut self) -> GameSpyPacketResult<(&'a str, &'a str)> {
        Ok((&self.shift_str(b'\\')?, &self.shift_str(b'\\')?))
    }

    fn end<T, E>(&mut self, val: E) -> Result<T, E> {
        self.src = &self.src[self.src.len()..];
        Err(val)
    }
}

impl <'a> Iterator for GameSpyPacketIterator<'a> {
    type Item = GameSpyPacketResult<(&'a str, &'a str)>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.src.len() {
            0 => None,
            _ => Some(self.read().or_else(|x| self.end(x)))
        }
    }
}

impl <'a> IntoIterator for &'a GameSpyPacket {
    type Item = GameSpyPacketResult<(&'a str, &'a str)>;
    type IntoIter = GameSpyPacketIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        GameSpyPacketIterator {
            src: &self.data[..self.data.len() - 7]
        }
    }
}

#[derive(Debug)]
pub struct GameSpyPacketBuilder<'a> {
    len: usize,
    buf: Vec<(&'a str)>
}

impl <'a> GameSpyPacketBuilder<'a> {
    pub fn new() -> GameSpyPacketBuilder<'a> {
        GameSpyPacketBuilder {
            len: 7,
            buf: Vec::new()
        }
    }

    pub fn push(&mut self, key: &'a str, value: &'a str) {
        self.len += 1 + key.len() + 1 + value.len();
        self.buf.push(key);
        self.buf.push(value);
    }

    pub fn build(self) -> GameSpyPacket {
        let mut buf: Vec<u8> = Vec::with_capacity(self.len);
        for item in &self.buf {
            buf.push(b'\\');
            buf.extend(item.as_bytes());
        }
        buf.extend(b"\\final\\");
        GameSpyPacket {
            data: buf.into_boxed_slice()
        }
    }
}
