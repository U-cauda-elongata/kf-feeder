pub mod jvcmusic_co_jp;
pub mod kadokawa_co_jp;
pub mod kemono_friends_sega_jp;

use std::future::Future;

use bytes::Bytes;
use futures::Stream;
use hyper::body::Sender;
use reqwest::Url;

pub trait Transcode {
    type Future: Future<Output = Result<(), Self::Error>>;
    type Error;

    fn transcode<I>(&self, url: Url, input: I, output: Sender) -> Self::Future
    where
        I: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin + 'static;
}
