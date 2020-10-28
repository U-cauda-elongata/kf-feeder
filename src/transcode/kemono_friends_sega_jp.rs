use std::{
    fmt::{self, Formatter},
    io::Write,
};

use futures::{future::FutureResult, IntoFuture, Stream};
use reqwest::Url;
use serde::{
    de::{self, DeserializeSeed, Error as _},
    Deserialize,
};
use xml::events::{BytesStart, BytesText, Event};

use crate::util::{feed, tag, IterRead};

pub struct Transcode;

impl super::Transcode for Transcode {
    type Future = FutureResult<(), failure::Error>;
    type Error = failure::Error;

    fn transcode<W: Write>(
        &self,
        _: &Url,
        input: reqwest::r#async::Decoder,
        output: W,
    ) -> Self::Future {
        let mut d = json::Deserializer::from_reader(IterRead::new(input.wait()));
        Transcoder::new(output)
            .deserialize(&mut d)
            .map_err(Into::into)
            .into_future()
    }
}

struct Transcoder<W: Write>(xml::Writer<W>);

impl<W: Write> Transcoder<W> {
    pub fn new(w: W) -> Self {
        Transcoder(xml::Writer::new(w))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Key {
    Articles,
    #[serde(other)]
    Other,
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

            fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
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

struct DeserializeArticles<W: Write>(xml::Writer<W>);

impl<'de, W: Write> DeserializeSeed<'de> for DeserializeArticles<W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<W: Write>(xml::Writer<W>);
        impl<'de, W: Write> de::Visitor<'de> for Visitor<W> {
            type Value = ();
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "an array")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
                feed(&mut self.0, |writer| {
                    tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                        writer
                            .write_event(&Event::Text(BytesText::from_escaped_str(
                                "けものフレンズ３",
                            )))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?;
                    writer
                        .write_event(Event::Empty(BytesStart::borrowed(
                            br#"link href="https://kemono-friends.sega.jp/""#,
                            4,
                        )))
                        .map_err(de::Error::custom)?;
                    // tag(writer, BytesStart::borrowed_name(b"updated"), |writer| Ok(()))?;
                    tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                        const ID: &str = "tag:ursus.cauda.elongata@gmail.com,2019:proxy:https://kemono-friends.sega.jp/";
                        writer
                            .write_event(Event::Text(BytesText::from_escaped_str(ID)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?;
                    while let Some(()) = a.next_element_seed(DeserializeArticle(writer))? {}
                    Ok(())
                })
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

    fn visit_map<A: de::MapAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Key {
            Id,
            Categories,
            Title,
            Date,
            Modified,
            #[serde(other)]
            Other,
        }

        tag(&mut self.0, BytesStart::borrowed_name(b"entry"), |writer| {
            while let Some(key) = a.next_key::<Key>()? {
                match key {
                    Key::Id => {
                        let id = a.next_value::<String>()?;
                        tag(writer, BytesStart::borrowed_name(b"id"), |writer| {
                            let id = format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:https://kemono-friends.sega.jp/news/{}/", id);
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&id)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?;
                        let mut link = BytesStart::borrowed_name(b"link");
                        let href = format!("https://kemono-friends.sega.jp/news/{}/", id);
                        link.push_attribute(("href", &*href));
                        writer
                            .write_event(&Event::Empty(link))
                            .map_err(de::Error::custom)?;
                    }
                    Key::Categories => {
                        for c in &a.next_value::<Vec<String>>()? {
                            let mut category = BytesStart::borrowed_name(b"category");
                            category.push_attribute(("term", &**c));
                            writer
                                .write_event(&Event::Empty(category))
                                .map_err(de::Error::custom)?;
                        }
                    }
                    Key::Title => tag(writer, BytesStart::borrowed_name(b"title"), |writer| {
                        let title = a.next_value::<String>()?;
                        writer
                            .write_event(&Event::Text(BytesText::from_plain_str(&title)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?,
                    Key::Date => tag(writer, BytesStart::borrowed_name(b"published"), |writer| {
                        let mut date = a.next_value::<String>()?;
                        date.push_str("+09:00");
                        writer
                            .write_event(&Event::Text(BytesText::from_plain_str(&date)))
                            .map_err(de::Error::custom)?;
                        Ok(())
                    })?,
                    Key::Modified => {
                        tag(writer, BytesStart::borrowed_name(b"updated"), |writer| {
                            let mut modified = a.next_value::<String>()?;
                            modified.push_str("+09:00");
                            writer
                                .write_event(&Event::Text(BytesText::from_plain_str(&modified)))
                                .map_err(de::Error::custom)?;
                            Ok(())
                        })?
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
