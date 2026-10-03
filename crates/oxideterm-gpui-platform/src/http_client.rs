use std::{sync::Arc, time::Duration};

use futures::{AsyncReadExt, future::BoxFuture};
use gpui::http_client::{
    AsyncBody, HttpClient, RedirectPolicy, Request, RequestTimeout, Response, Url,
    http::HeaderValue,
};
use tokio_util::task::AbortOnDropHandle;

/// Bridges GPUI asset requests to the application's existing Tokio runtime.
pub struct AssetHttpClient {
    runtime: Arc<tokio::runtime::Runtime>,
    redirects: reqwest::Client,
    no_redirects: reqwest::Client,
}

impl AssetHttpClient {
    pub fn new(runtime: Arc<tokio::runtime::Runtime>) -> anyhow::Result<Self> {
        let client = |redirect| {
            reqwest::Client::builder()
                .redirect(redirect)
                .timeout(Duration::from_secs(30))
                .build()
        };
        Ok(Self {
            runtime,
            redirects: client(reqwest::redirect::Policy::limited(10))?,
            no_redirects: client(reqwest::redirect::Policy::none())?,
        })
    }
}

impl HttpClient for AssetHttpClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let timeout = request
            .extensions()
            .get::<RequestTimeout>()
            .map_or(Duration::from_secs(30), |timeout| timeout.0);
        let client = match request.extensions().get::<RedirectPolicy>() {
            Some(RedirectPolicy::FollowAll) => Ok(self.redirects.clone()),
            Some(RedirectPolicy::FollowLimit(limit)) => reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(*limit as usize))
                .timeout(Duration::from_secs(30))
                .build(),
            _ => Ok(self.no_redirects.clone()),
        };
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let client = client.map_err(reqwest::Error::without_url)?;
            // GPUI polls assets outside Tokio. Keep the runtime alive for the request
            // and cancel network work when GPUI drops the asset future.
            AbortOnDropHandle::new(runtime.spawn(async move {
                tokio::time::timeout(timeout, async move {
                    let (parts, mut body) = request.into_parts();
                    let has_body = !matches!(&body.0, gpui::http_client::Inner::Empty);
                    let mut bytes = zeroize::Zeroizing::new(Vec::new());
                    body.read_to_end(&mut bytes)
                        .await
                        .map_err(|_| anyhow::anyhow!("Unable to read asset HTTP request body"))?;
                    // Reqwest owns the protocol buffers; the bridge's temporary body is cleared on drop.
                    let mut request = client
                        .request(parts.method, parts.uri.to_string())
                        .headers(parts.headers)
                        .version(parts.version)
                        .timeout(timeout);
                    if has_body {
                        request = request.body(bytes.to_vec());
                    }
                    let request = request.build().map_err(reqwest::Error::without_url)?;
                    drop(bytes);
                    let response = client
                        .execute(request)
                        .await
                        .map_err(reqwest::Error::without_url)?;
                    let status = response.status();
                    let version = response.version();
                    let headers = response.headers().clone();
                    let body = response
                        .bytes()
                        .await
                        .map_err(reqwest::Error::without_url)?
                        .to_vec();
                    let mut response = Response::builder()
                        .status(status)
                        .version(version)
                        .body(AsyncBody::from(body))?;
                    *response.headers_mut() = headers;
                    Ok(response)
                })
                .await
                .map_err(|_| anyhow::anyhow!("Asset HTTP request timed out"))?
            }))
            .await?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::http_client::HttpRequestExt;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn asset_requests_work_outside_tokio_and_respect_redirects() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let (cancel_started, cancel_received) = std::sync::mpsc::sync_channel(1);
        let image = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"></svg>";
        let worker = std::thread::spawn(move || {
            for expected_path in [
                "/image.svg",
                "/redirect",
                "/redirect",
                "/image.svg",
                "/payload",
                "/slow",
                "/cancel",
            ] {
                let (mut connection, _) = server.accept().unwrap();
                connection
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    connection.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let first_line = std::str::from_utf8(&request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap();
                let method = if expected_path == "/payload" {
                    "POST"
                } else {
                    "GET"
                };
                assert_eq!(first_line, format!("{method} {expected_path} HTTP/1.1"));
                if expected_path == "/payload" {
                    assert!(
                        std::str::from_utf8(&request)
                            .unwrap()
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .any(|(name, value)| name.eq_ignore_ascii_case("x-asset-request")
                                && value.trim() == "yes")
                    );
                    let mut body = [0; 13];
                    connection.read_exact(&mut body).unwrap();
                    assert_eq!(&body, b"asset-request");
                }
                if expected_path == "/slow" || expected_path == "/cancel" {
                    if expected_path == "/cancel" {
                        cancel_started.send(()).unwrap();
                    }
                    // The deadline must cancel the socket, not just stop waiting for its result.
                    assert_eq!(connection.read(&mut byte).unwrap(), 0);
                    continue;
                }
                if expected_path == "/redirect" {
                    connection.write_all(b"HTTP/1.1 302 Found\r\nLocation: /image.svg\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(connection, "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", image.len()).unwrap();
                    connection.write_all(image).unwrap();
                }
            }
        });
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        // Ambient proxies can rewrite headers and retain origin sockets after client cancellation.
        let direct_client = |policy| {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(policy)
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap()
        };
        let client = AssetHttpClient {
            runtime,
            redirects: direct_client(reqwest::redirect::Policy::limited(10)),
            no_redirects: direct_client(reqwest::redirect::Policy::none()),
        };
        for (path, follow, expected_status, expected_body) in [
            ("/image.svg", true, 200, image.as_slice()),
            ("/redirect", false, 302, b"".as_slice()),
            ("/redirect", true, 200, image.as_slice()),
        ] {
            let mut response = futures::executor::block_on(client.get(
                &format!("http://{address}{path}"),
                AsyncBody::empty(),
                follow,
            ))
            .unwrap();
            assert_eq!(response.status().as_u16(), expected_status);
            let mut body = Vec::new();
            futures::executor::block_on(response.body_mut().read_to_end(&mut body)).unwrap();
            assert_eq!(body, expected_body);
        }
        let request = Request::builder()
            .uri(format!("http://{address}/payload"))
            .method("POST")
            .header("x-asset-request", "yes")
            .body(AsyncBody::from("asset-request"))
            .unwrap();
        let mut response = futures::executor::block_on(client.send(request)).unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.headers()["content-type"], "image/svg+xml");
        let mut body = Vec::new();
        futures::executor::block_on(response.body_mut().read_to_end(&mut body)).unwrap();
        assert_eq!(body, image);
        let request = Request::builder()
            .uri(format!("http://{address}/slow"))
            .timeout(Duration::from_millis(100))
            .body(AsyncBody::empty())
            .unwrap();
        let error = futures::executor::block_on(client.send(request))
            .err()
            .expect("deadline should expire");
        assert!(format!("{error:#}").contains("timed out"));
        let mut pending = client.get(
            &format!("http://{address}/cancel"),
            AsyncBody::empty(),
            false,
        );
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);
        assert!(pending.as_mut().poll(&mut context).is_pending());
        cancel_received
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        drop(pending);
        worker.join().unwrap();
        let error = futures::executor::block_on(client.get(
            &format!("http://{address}/image.svg?token=asset-test-token"),
            AsyncBody::empty(),
            false,
        ))
        .err()
        .expect("closed listener should reject request");
        assert!(!format!("{error:?}").contains("asset-test-token"));
    }
}
