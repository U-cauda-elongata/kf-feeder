pub mod kadokawa_co_jp;
pub mod kemono_friends_sega_jp;

use std::io::Write;

use futures::Future;
use reqwest::Url;

pub trait Transcode {
    type Future: Future<Item = (), Error = Self::Error>;
    type Error;

    fn transcode<W: Write>(
        &self,
        url: &Url,
        input: reqwest::r#async::Decoder,
        output: W,
    ) -> Self::Future;
}
