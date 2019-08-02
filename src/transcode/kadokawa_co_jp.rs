use std::{
    fmt::{self, Formatter},
    io::Write,
};

use futures::{future::FutureResult, IntoFuture, Stream};
use reqwest::Url;
use serde::{
    de::{self, DeserializeSeed},
    Deserialize,
};
use xml::events::{BytesStart, BytesText, Event};

use crate::util::IterRead;

pub struct Transcode;

impl super::Transcode for Transcode {
    type Future = FutureResult<(), failure::Error>;
    type Error = failure::Error;

    fn transcode<W: Write>(
        &self,
        url: &Url,
        input: reqwest::r#async::Decoder,
        output: W,
    ) -> Self::Future {
        let mut d = json::Deserializer::from_reader(IterRead::new(input.wait()));
        Transcoder::new(output, url)
            .deserialize(&mut d)
            .map_err(Into::into)
            .into_future()
    }
}

struct Transcoder<'a, W: Write>(xml::Writer<W>, &'a Url);

impl<'a, W: Write> Transcoder<'a, W> {
    pub fn new(w: W, url: &'a Url) -> Self {
        Transcoder(xml::Writer::new(w), url)
    }
}

impl<'a, 'de, W: Write> DeserializeSeed<'de> for Transcoder<'a, W> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct Visitor<'a, W: Write>(xml::Writer<W>, &'a Url);
        impl<'a, 'de, W: Write> de::Visitor<'de> for Visitor<'a, W> {
            type Value = ();

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "a sequence")
            }

            fn visit_seq<A: de::SeqAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
                feed!(self.0 => {
                    tag!(self.0, BytesStart::borrowed_name(b"title") => {
                        self.0.write_event(&Event::Text(
                            BytesText::from_escaped_str("検索結果一覧 | KADOKAWA")
                        )).map_err(de::Error::custom)?;
                    });
                    let mut href_: String;
                    let href = if let Some(q) = self.1.query() {
                        href_ =
                            "https://www.kadokawa.co.jp/product/search/?".into();
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
                    self.0
                        .write_event(Event::Empty(BytesStart::owned(link, 4)))
                        .map_err(de::Error::custom)?;
                    // tag!(self.0, BytesStart::borrowed_name(b"updated") => {});
                    tag!(self.0, BytesStart::borrowed_name(b"id") => {
                        let id =
                            format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:{}", href);
                        self.0.write_event(
                            Event::Text(BytesText::from_escaped_str(id))
                        ).map_err(de::Error::custom)?;
                    });
                    while let Some(()) = a.next_element_seed(DeserializeEntry(&mut self.0))? {}
                });

                Ok(())
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

    fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
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

        tag!(self.0, BytesStart::borrowed_name(b"entry") => {
            while let Some(key) = a.next_key::<Key>()? {
                match key {
                    Key::ItemCode => {
                        let code = a.next_value::<String>()?;
                        tag!(self.0, BytesStart::borrowed_name(b"id") => {
                            let id = format!("tag:ursus.cauda.elongata@gmail.com,2019:proxy:https://www.kadokawa.co.jp/product/{}/", code);
                            self.0.write_event(&Event::Text(BytesText::from_plain_str(&id)))
                                .map_err(de::Error::custom)?;
                        });
                        let mut link = BytesStart::borrowed_name(b"link");
                        let href = format!("https://www.kadokawa.co.jp/product/{}/", code);
                        link.push_attribute(("href", &*href));
                        self.0.write_event(&Event::Empty(link)).map_err(de::Error::custom)?;
                    }
                    Key::Title => tag!(self.0, BytesStart::borrowed_name(b"title") => {
                        let title = a.next_value::<String>()?;
                        self.0.write_event(&Event::Text(BytesText::from_plain_str(&title)))
                            .map_err(de::Error::custom)?;
                    }),
                    Key::Catch => {
                        tag!(self.0, BytesStart::borrowed(br#"content type="text""#, 7) => {
                            let text = a.next_value::<String>()?;
                            self.0.write_event(&Event::CData(BytesText::from_plain_str(&text)))
                                .map_err(de::Error::custom)?;
                        });
                    }
                    Key::Author2 => tag!(self.0, BytesStart::borrowed_name(b"author") => {
                        tag!(self.0, BytesStart::borrowed_name(b"name") => {
                            let name = a.next_value::<String>()?;
                            self.0.write_event(&Event::Text(BytesText::from_plain_str(&name)))
                                .map_err(de::Error::custom)?;
                        });
                    }),
                    Key::PublicationDate => {
                        let mut date = a.next_value::<String>()?;
                        date.push_str("T00:00:00+09:00");
                        tag!(self.0, BytesStart::borrowed_name(b"published") => {
                            self.0.write_event(&Event::Text(BytesText::from_escaped_str(&date)))
                                .map_err(de::Error::custom)?;
                        });
                        tag!(self.0, BytesStart::borrowed_name(b"updated") => {
                            self.0.write_event(&Event::Text(BytesText::from_escaped_str(&date)))
                                .map_err(de::Error::custom)?;
                        });
                    }
                    Key::Other => {
                        a.next_value::<de::IgnoredAny>()?;
                    }
                }
            }
        });

        Ok(())
    }
}
