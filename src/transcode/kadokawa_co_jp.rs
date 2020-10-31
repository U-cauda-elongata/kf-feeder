use std::{
    fmt::{self, Formatter},
    io::Write,
    marker::Unpin,
};

use bytes::Bytes;
use futures::Stream;
use hyper::body::Sender;
use reqwest::Url;
use serde::{
    de::{self, DeserializeSeed},
    Deserialize,
};
use xml::events::{BytesStart, BytesText, Event};

use crate::util::*;

pub struct Transcode;

impl super::Transcode for Transcode {
    type Future = JoinHandle<json::Result<()>>;
    type Error = json::Error;

    fn transcode<I>(&self, url: Url, input: I, output: Sender) -> Self::Future
    where
        I: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin + 'static,
    {
        let mut d = json::Deserializer::from_reader(StreamRead::new(input));
        let t = Transcoder::new(BodyWrite::new(output), url);
        JoinHandle(tokio::task::spawn_blocking(move || t.deserialize(&mut d)))
    }
}

struct Transcoder<W: Write>(xml::Writer<W>, Url);

impl<W: Write> Transcoder<W> {
    pub fn new(w: W, url: Url) -> Self {
        Transcoder(xml::Writer::new(w), url)
    }
}

impl<'de, W: Write> DeserializeSeed<'de> for Transcoder<W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<W: Write>(xml::Writer<W>, Url);
        impl<'de, W: Write> de::Visitor<'de> for Visitor<W> {
            type Value = ();

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "a sequence")
            }

            fn visit_seq<A: de::SeqAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
                let uri = self.1;
                feed(&mut self.0, |writer| {
                    tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                        writer
                            .write_event(&Event::Text(BytesText::from_escaped_str(
                                "検索結果一覧 | KADOKAWA",
                            )))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?;
                    let mut href_: String;
                    let href = if let Some(q) = uri.query() {
                        href_ = "https://www.kadokawa.co.jp/product/search/?".into();
                        let mut first = true;
                        for pair in q.split('&') {
                            if !pair.starts_with("id=") {
                                if first {
                                    first = false;
                                } else {
                                    href_.push_str("&amp;");
                                }
                                href_.push_str(pair);
                            }
                        }
                        &href_
                    } else {
                        "https://www.kadokawa.co.jp/product/search/"
                    };
                    let link = format!(r#"link href="{}""#, href);
                    writer
                        .write_event(Event::Empty(BytesStart::owned(link, 4)))
                        .map_err(de::Error::custom)?;
                    // tag(writer, BytesStart::borrowed_name(b"updated"), |writer| Ok(()))?;
                    tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                        let id = format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:{}", href);
                        writer
                            .write_event(Event::Text(BytesText::from_escaped_str(id)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?;
                    while let Some(()) = a.next_element_seed(DeserializeEntry(writer))? {}
                    Ok(())
                })
            }
        }

        d.deserialize_seq(Visitor(self.0, self.1))
    }
}

struct DeserializeEntry<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'de, 'a, W: Write> DeserializeSeed<'de> for DeserializeEntry<'a, W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(EntryVisitor(self.0))
    }
}

struct EntryVisitor<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'de, 'a, W: Write> de::Visitor<'de> for EntryVisitor<'a, W> {
    type Value = ();

    fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "an object")
    }

    fn visit_map<A: de::MapAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        enum Key {
            ItemCode,
            Title,
            Catch,
            Author2,
            PublicationDate,
            #[serde(other)]
            Other,
        }

        tag(&mut self.0, BytesStart::borrowed_name(b"entry"), |writer| {
            while let Some(key) = a.next_key::<Key>()? {
                match key {
                    Key::ItemCode => {
                        let code = a.next_value::<String>()?;
                        tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                            let id = format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:https://www.kadokawa.co.jp/product/{}/", code);
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&id)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                        let mut link = BytesStart::borrowed_name(b"link");
                        let href = format!("https://www.kadokawa.co.jp/product/{}/", code);
                        link.push_attribute(("href", &*href));
                        writer
                            .write_event(&Event::Empty(link))
                            .map_err(de::Error::custom)?;
                    }
                    Key::Title => tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                        let title = a.next_value::<String>()?;
                        writer
                            .write_event(&Event::Text(BytesText::from_plain_str(&title)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?,
                    Key::Catch => {
                        let start = BytesStart::borrowed(br#"content type="text""#, 7);
                        tag(writer, start, |writer| {
                            let text = a.next_value::<String>()?;
                            writer
                                .write_event(&Event::CData(BytesText::from_plain_str(&text)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                    }
                    Key::Author2 => tag(writer, BytesStart::borrowed_name(b"author"), |writer| {
                        tag(writer, BytesStart::borrowed_name(b"name"), |writer| {
                            let name = a.next_value::<String>()?;
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&name)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })
                    })?,
                    Key::PublicationDate => {
                        let mut date = a.next_value::<String>()?;
                        date.push_str("T00:00:00+09:00");
                        tag(writer, BytesStart::borrowed_name(b"published"), |writer| {
                            writer
                                .write_event(&Event::Text(BytesText::from_escaped_str(&date)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                        tag(writer, BytesStart::borrowed_name(b"updated"), |writer| {
                            writer
                                .write_event(&Event::Text(BytesText::from_escaped_str(&date)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                    }
                    Key::Other => {
                        a.next_value::<de::IgnoredAny>()?;
                    }
                }
            }
            Ok(())
        })
    }
}
