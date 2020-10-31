use std::{
    error::Error,
    future::Future,
    io::{self, Read, Write},
    marker::Unpin,
    ops::Deref,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::{Stream, StreamExt};
use hyper::body::Sender;
use serde::de;
use xml::events::{BytesDecl, BytesEnd, BytesStart, Event};

pub struct BodyWrite(Sender);

impl BodyWrite {
    pub fn new(sender: Sender) -> Self {
        BodyWrite(sender)
    }
}

impl Write for BodyWrite {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        futures::executor::block_on(self.0.send_data(Bytes::copy_from_slice(b)))
            .map(|()| b.len())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct StreamRead<S, B> {
    stream: S,
    buf: B,
    pos: usize,
}

impl<S, B, E> StreamRead<S, B>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: Deref<Target = [u8]> + Default,
    E: Error + Send + Sync + 'static,
{
    pub fn new(stream: S) -> Self {
        StreamRead {
            stream,
            buf: B::default(),
            pos: 0,
        }
    }
}

impl<S, B, E> Read for StreamRead<S, B>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: Deref<Target = [u8]>,
    E: Error + Send + Sync + 'static,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.buf.len() <= self.pos {
            if let Some(b) = futures::executor::block_on(self.stream.next()) {
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

pub struct JoinHandle<T>(pub tokio::task::JoinHandle<T>);

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map(|result| match result {
            Ok(t) => t,
            Err(e) => std::panic::resume_unwind(e.try_into_panic().unwrap()),
        })
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
