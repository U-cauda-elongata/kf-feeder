#[macro_use]
mod util;

mod router;
mod transcode;

use std::{convert::Infallible, net::ToSocketAddrs};

use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use reqwest::Client;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "kf-pipitor",
    about = "A Web application that converts Kemono Friends-related Web pages to Atom feeds."
)]
struct Opt {
    /// Host name for the HTTP server
    #[structopt(default_value = "127.0.0.1")]
    host: String,
    /// Port number for the HTTP server
    #[structopt(short = "p", long = "port", default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> hyper::Result<()> {
    let opt = Opt::from_args();
    let addr = (&*opt.host, opt.port)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    let client = Client::builder().referer(false).build().unwrap();
    let new_service = make_service_fn(|_| {
        let client = client.clone();
        let service = service_fn(move |request| router::route(request, client.clone()));
        async { Ok::<_, Infallible>(service) }
    });
    Server::bind(&addr).serve(new_service).await
}
