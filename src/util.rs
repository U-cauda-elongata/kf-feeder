use std::{
    error::Error,
    io::{self, Read, Write},
    ops::Deref,
};

use futures::sink::{Sink, Wait};
use serde::de;
use xml::events::{BytesDecl, BytesEnd, BytesStart, Event};

pub struct IterRead<I, B> {
    iter: I,
    buf: B,
    pos: usize,
}

impl<I, B, E> IterRead<I, B>
where
    I: Iterator<Item = Result<B, E>>,
    B: Deref<Target = [u8]> + Default,
    E: Error + Send + Sync + 'static,
{
    pub fn new(iter: I) -> Self {
        IterRead {
            iter,
            buf: B::default(),
            pos: 0,
        }
    }
}

impl<I, B, E> Read for IterRead<I, B>
where
    I: Iterator<Item = Result<B, E>>,
    B: Deref<Target = [u8]>,
    E: Error + Send + Sync + 'static,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.buf.len() <= self.pos {
            if let Some(b) = self.iter.next() {
                self.buf = b.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                self.pos = 0;
            } else {
                return Ok(0);
            }
        }

        let len = (&self.buf[self.pos..]).read(buf)?;
        self.pos += len;
        Ok(len)
    }
}

pub struct SinkWrite<S>(Wait<S>);

impl<S: Sink> SinkWrite<S> {
    pub fn new(sink: S) -> Self {
        SinkWrite(sink.wait())
    }

    pub fn close(&mut self) -> Result<(), S::SinkError> {
        self.0.close()
    }
}

impl<S: Sink> Write for SinkWrite<S>
where
    S::SinkItem: From<Vec<u8>>,
    S::SinkError: Error + Send + Sync + 'static,
{
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0
            .send(b.to_vec().into())
            .map(|()| b.len())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

pub fn tag<W, F, E>(writer: &mut xml::Writer<W>, start: BytesStart, body: F) -> Result<(), E>
where
    W: Write,
    F: FnOnce(&mut xml::Writer<W>) -> Result<(), E>,
    E: de::Error,
{
    let start = Event::Start(start);
    let end = if let Event::Start(ref tag) = start {
        xml::events::Event::End(BytesEnd::borrowed(tag.name()))
    } else {
        unreachable!();
    };

    writer.write_event(&start).map_err(E::custom)?;
    body(writer)?;
    writer.write_event(&end).map_err(E::custom)?;

    Ok(())
}

pub fn feed<W, F, E>(writer: &mut xml::Writer<W>, body: F) -> Result<(), E>
where
    W: Write,
    F: FnOnce(&mut xml::Writer<W>) -> Result<(), E>,
    E: de::Error,
{
    writer
        .write_event(&Event::Decl(BytesDecl::new(b"1.0", Some(b"utf-8"), None)))
        .map_err(de::Error::custom)?;
    let start = BytesStart::borrowed(br#"feed xmlns="http://www.w3.org/2005/Atom""#, 4);
    tag(writer, start, body)
}
