use std::{fmt::Display, io::Write, mem};

use futures::future::{lazy, Future, IntoFuture};
use hyper::{
    header::{HeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST, RANGE},
    Body, Request, Response, StatusCode,
};
use reqwest::r#async::{Client, Request as Reqwest};

use crate::{
    transcode::{self, Transcode},
    util::SinkWrite,
};

#[auto_enums::auto_enum(futures01::Future)]
pub fn route(
    request: Request<Body>,
    client: &Client,
) -> impl Future<Item = Response<Body>, Error = failure::Error> {
    let not_found = lazy(|| {
        let body = "Not found";
        Ok(Response::builder()
            .header(CONTENT_LENGTH, body.len() as u64)
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(body))
            .unwrap())
    });

    let parts = request.into_parts().0;

    match parts.method {
        hyper::Method::GET | hyper::Method::HEAD => {}
        _ => return not_found,
    }

    let url_str = match parts.uri.path_and_query() {
        None => return not_found,
        Some(ref paq) if !paq.as_str().starts_with("/") => return not_found,
        Some(paq) => &paq.as_str()[1..],
    };
    let url = match url_str.parse() {
        Ok(url) => url,
        Err(_) => return not_found,
    };

    let mut reqwest = Reqwest::new(parts.method, url);
    *reqwest.headers_mut() = parts.headers;

    match url_str {
        "https://kemono-friends.sega.jp/news/articles.json" => proxy_response(
            client,
            reqwest,
            transcode::kemono_friends_sega_jp::Transcode,
        ),
        _ => not_found,
    }
}

fn proxy_response<T>(
    client: &Client,
    mut reqw: Reqwest,
    transcode: T,
) -> impl Future<Item = Response<Body>, Error = failure::Error>
where
    T: Transcode + Send + 'static,
    T::Future: Send + 'static,
    T::Error: Display,
{
    let head = reqw.method() == reqwest::Method::HEAD;

    reqw.headers_mut().remove(HOST);
    reqw.headers_mut().remove(RANGE);
    reqw.headers_mut().remove(ACCEPT_ENCODING);

    let fut = client.execute(reqw).map_err(Into::into);
    fut.and_then(move |mut resw| {
        let mut res = Response::builder();
        let headers = res.headers_mut().unwrap();
        mem::swap(headers, resw.headers_mut());

        match resw.status() {
            StatusCode::OK => {
                let body = if head {
                    Body::default()
                } else {
                    let (tx, body) = Body::channel();
                    tokio::spawn(lazy(move || {
                        let mut w = SinkWrite::new(tx);
                        transcode
                            .transcode(resw.into_body(), &mut w)
                            .into_future()
                            .map_err(eprintln)
                            .and_then(move |()| {
                                w.flush().map_err(eprintln)?;
                                w.close().map_err(eprintln)
                            })
                    }));
                    body
                };

                headers.remove(CONNECTION);
                headers.remove(CONTENT_LENGTH);
                let content_type = "application/atom+xml;charset=UTF-8";
                headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));

                Ok(res.body(body)?)
            }
            s => Ok(res.status(s).body(Body::default())?),
        }
    })
}

fn eprintln<T: Display>(t: T) {
    eprintln!("{}", t);
}
