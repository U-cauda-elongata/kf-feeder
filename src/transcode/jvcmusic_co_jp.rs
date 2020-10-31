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
    de::{self, DeserializeSeed, Error as _},
    Deserialize,
};
use xml::events::{BytesStart, BytesText, Event};

use crate::util::*;

pub struct Transcode;

impl super::Transcode for Transcode {
    type Future = JoinHandle<json::Result<()>>;
    type Error = json::Error;

    fn transcode<I>(&self, _: Url, input: I, output: Sender) -> Self::Future
    where
        I: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin + 'static,
    {
        let mut d = json::Deserializer::from_reader(StreamRead::new(input));
        let t = Transcoder::new(BodyWrite::new(output));
        JoinHandle(tokio::task::spawn_blocking(move || t.deserialize(&mut d)))
    }
}

struct Transcoder<W: Write>(xml::Writer<W>);

impl<W: Write> Transcoder<W> {
    pub fn new(w: W) -> Self {
        Transcoder(xml::Writer::new(w))
    }
}

impl<'de, W: Write> DeserializeSeed<'de> for Transcoder<W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<W: Write>(xml::Writer<W>);
        impl<'de, W: Write> de::Visitor<'de> for Visitor<W> {
            type Value = ();

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "an object")
            }

            fn visit_map<A: de::MapAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum Key {
                    Title,
                    Description,
                    Url,
                    Contents,
                    #[serde(other)]
                    Other,
                }

                feed(&mut self.0, |writer| {
                    while let Some(key) = a.next_key::<Key>()? {
                        match key {
                            Key::Title => {
                                let title = a.next_value::<String>()?;
                                tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                                    writer
                                        .write_event(&Event::Text(BytesText::from_plain_str(
                                            &title,
                                        )))
                                        .map_err(de::Error::custom)?;
                                    Ok(())
                                })?;
                            }
                            Key::Description => {
                                let description = a.next_value::<String>()?;
                                tag(writer, BytesStart::borrowed_name(b"subtitle"), |writer| {
                                    writer
                                        .write_event(&Event::Text(BytesText::from_plain_str(
                                            &description,
                                        )))
                                        .map_err(de::Error::custom)?;
                                    Ok(())
                                })?;
                            }
                            Key::Url => {
                                let href = a.next_value::<String>()?;
                                let link = format!(r#"link href="{}""#, href);
                                writer
                                    .write_event(Event::Empty(BytesStart::owned(link, 4)))
                                    .map_err(de::Error::custom)?;
                                tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                                    let id = format!(
                                        "tag:ursus.cauda.elongata@gmail.com,2019:proxy:{}",
                                        href
                                    );
                                    writer
                                        .write_event(Event::Text(BytesText::from_escaped_str(id)))
                                        .map_err(de::Error::custom)?;
                                    Ok(())
                                })?;
                            }
                            Key::Contents => a.next_value_seed(DeserializeContents(writer))?,
                            Key::Other => {
                                a.next_value::<de::IgnoredAny>()?;
                            }
                        }
                    }
                    Ok(())
                })
            }
        }

        d.deserialize_map(Visitor(self.0))
    }
}

struct DeserializeContents<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'a, 'de, W: Write> DeserializeSeed<'de> for DeserializeContents<'a, W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<'a, W: Write>(&'a mut xml::Writer<W>);
        impl<'a, 'de, W: Write> de::Visitor<'de> for Visitor<'a, W> {
            type Value = ();

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "an object")
            }

            fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum Key {
                    Articles,
                    #[serde(other)]
                    Other,
                }

                while let Some(key) = a.next_key::<Key>()? {
                    if let Key::Articles = key {
                        return a
                            .next_value_seed(DeserializeArticles(self.0))
                            .and_then(|()| {
                                while let Some((de::IgnoredAny, de::IgnoredAny)) = a.next_entry()? {
                                }
                                Ok(())
                            });
                    } else {
                        a.next_value::<de::IgnoredAny>()?;
                    }
                }

                Err(A::Error::missing_field("articles"))
            }
        }

        d.deserialize_map(Visitor(self.0))
    }
}

struct DeserializeArticles<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'a, 'de, W: Write> DeserializeSeed<'de> for DeserializeArticles<'a, W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<'a, W: Write>(&'a mut xml::Writer<W>);
        impl<'a, 'de, W: Write> de::Visitor<'de> for Visitor<'a, W> {
            type Value = ();
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "an array")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
                while let Some(()) = a.next_element_seed(DeserializeArticle(self.0))? {}
                Ok(())
            }
        }

        d.deserialize_seq(Visitor(self.0))
    }
}

struct DeserializeArticle<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'de, 'a, W: Write> DeserializeSeed<'de> for DeserializeArticle<'a, W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(ArticleVisitor(self.0))
    }
}

struct ArticleVisitor<'a, W: Write>(&'a mut xml::Writer<W>);

impl<'de, 'a, W: Write> de::Visitor<'de> for ArticleVisitor<'a, W> {
    type Value = ();

    fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "a map")
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Key {
            Url,
            Title,
            Text,
            OpenDt,
            #[serde(other)]
            Other,
        }

        tag(self.0, BytesStart::borrowed_name(b"entry"), |writer| {
            while let Some(key) = a.next_key::<Key>()? {
                match key {
                    Key::Url => {
                        let url = a.next_value::<String>()?;
                        tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                            let id =
                                format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:{}", url);
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&id)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                        let link = format!(r#"link href="{}""#, url);
                        writer
                            .write_event(Event::Empty(BytesStart::owned(link, 4)))
                            .map_err(de::Error::custom)?;
                    }
                    Key::Title => tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                        let title = a.next_value::<String>()?;
                        writer
                            .write_event(&Event::Text(BytesText::from_plain_str(&title)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?,
                    Key::Text => {
                        let text = a.next_value::<String>()?;
                        let start = BytesStart::borrowed(br#"content type="text""#, 7);
                        tag(writer, start, |writer| {
                            writer
                                .write_event(&Event::CData(BytesText::from_plain_str(&text)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                    }
                    Key::OpenDt => {
                        let mut date = a.next_value::<String>()?;
                        if date.get(10..11) != Some(" ") {
                            return Err(de::Error::custom("unrecognized `open_dt`"));
                        }
                        date.replace_range(10..11, "T");
                        date.push_str("+09:00");
                        tag(writer, BytesStart::borrowed_name(b"published"), |writer| {
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&date)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                        tag(writer, BytesStart::borrowed_name(b"updated"), |writer| {
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&date)))
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
