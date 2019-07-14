pub mod kemono_friends_sega_jp;

use std::io::Write;

use futures::Future;

pub trait Transcode {
    type Future: Future<Item = (), Error = Self::Error>;
    type Error;

    fn transcode<W: Write>(&self, input: reqwest::r#async::Decoder, output: W) -> Self::Future;
}
