#[macro_use]
mod util;

mod router;
mod transcode;

use std::net::ToSocketAddrs;

use futures::future::Future;
use hyper::{service::service_fn, Server};
use reqwest::r#async::Client;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "A Web application that converts Kemono Friends-related Web pages to Atom feeds.")]
struct Opt {
    /// Host name for the HTTP server
    #[structopt(default_value = "127.0.0.1")]
    host: String,
    /// Port number for the HTTP server
    #[structopt(short = "p", long = "port", default_value = "8080")]
    port: u16,
}

fn main() {
    let opt = Opt::from_args();
    let addr = (&*opt.host, opt.port)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    let server = Server::bind(&addr)
        .serve(|| {
            let client = Client::builder().referer(false).build().unwrap();
            service_fn(move |req| router::route(req, &client))
        })
        .map_err(|e| eprintln!("Error: {}", e));
    hyper::rt::run(server)
}
