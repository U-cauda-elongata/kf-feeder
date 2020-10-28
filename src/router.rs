use std::{fmt::Display, io::Write, mem};

use futures::future::{lazy, Future, IntoFuture};
use hyper::{
    header::{HeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST, RANGE},
    Body, Request, Response, StatusCode,
};
use reqwest::r#async::{Client, Decoder, Request as Reqwest, Response as Reswponse};

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

    let mut parts = request.into_parts().0;

    let head = match parts.method {
        hyper::Method::HEAD => true,
        hyper::Method::GET => false,
        _ => return not_found,
    };

    let url: url::Url = match parts.uri.path_and_query() {
        None => return not_found,
        Some(ref paq) if !paq.as_str().starts_with('/') => return not_found,
        Some(paq) => match paq.as_str()[1..].parse() {
            Ok(url) => url,
            Err(_) => return not_found,
        },
    };

    parts.headers.remove(HOST);
    parts.headers.remove(RANGE);
    parts.headers.remove(ACCEPT_ENCODING);
    let reqwest = |url| {
        let mut reqwest = Reqwest::new(parts.method, url);
        *reqwest.headers_mut() = parts.headers;
        client.execute(reqwest).map_err(Into::into)
    };

    match &url[..url::Position::AfterPath] {
        "https://kemono-friends.sega.jp/news/articles.json" => {
            return reqwest(url).and_then(move |resw| {
                proxy_response(transcode::kemono_friends_sega_jp::Transcode, resw, head)
            })
        }
        "https://www.kadokawa.co.jp/json.jsp" => {
            if let Some(q) = url.query() {
                for pair in q.split('&') {
                    if pair.starts_with("id=") && pair[3..] == *"342" {
                        return reqwest(url).and_then(move |resw| {
                            proxy_response(transcode::kadokawa_co_jp::Transcode, resw, head)
                        });
                    }
                }
            }
        }
        _ => {}
    }

    return not_found;
}

fn proxy_response<T>(
    transcode: T,
    mut resw: Reswponse,
    head: bool,
) -> Result<Response<Body>, failure::Error>
where
    T: Transcode + Send + 'static,
    T::Future: Send + 'static,
    T::Error: Display,
{
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
                    let body = mem::replace(resw.body_mut(), Decoder::empty());
                    let mut w = SinkWrite::new(tx);
                    transcode
                        .transcode(resw.url(), body, &mut w)
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
}

fn eprintln<T: Display>(t: T) {
    eprintln!("{}", t);
}
