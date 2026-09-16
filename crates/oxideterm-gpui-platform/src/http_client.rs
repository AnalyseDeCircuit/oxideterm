use std::{sync::Arc, time::Duration};

use futures::future::BoxFuture;
use gpui::http_client::{HttpClient, HttpResponse};
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
    fn get(
        &self,
        url: &str,
        follow_redirects: bool,
    ) -> BoxFuture<'static, anyhow::Result<HttpResponse>> {
        let client = if follow_redirects {
            &self.redirects
        } else {
            &self.no_redirects
        };
        let request = client.get(url).build();
        let client = client.clone();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let request = request.map_err(reqwest::Error::without_url)?;
            // GPUI polls assets outside Tokio. Keep the runtime alive for the request
            // and cancel network work when GPUI drops the asset future.
            AbortOnDropHandle::new(runtime.spawn(async move {
                let response = client
                    .execute(request)
                    .await
                    .map_err(reqwest::Error::without_url)?;
                let status = response.status();
                let body = response
                    .bytes()
                    .await
                    .map_err(reqwest::Error::without_url)?
                    .to_vec();
                Ok(HttpResponse { status, body })
            }))
            .await?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn asset_requests_work_outside_tokio_and_respect_redirects() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let image = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"></svg>";
        let worker = std::thread::spawn(move || {
            for expected_path in ["/image.svg", "/redirect", "/redirect", "/image.svg"] {
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
                assert_eq!(first_line, format!("GET {expected_path} HTTP/1.1"));
                if expected_path == "/redirect" {
                    connection.write_all(b"HTTP/1.1 302 Found\r\nLocation: /image.svg\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(connection, "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", image.len()).unwrap();
                    connection.write_all(image).unwrap();
                }
            }
        });
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let client = AssetHttpClient::new(runtime).unwrap();
        for (path, follow, expected_status, expected_body) in [
            ("/image.svg", true, 200, image.as_slice()),
            ("/redirect", false, 302, b"".as_slice()),
            ("/redirect", true, 200, image.as_slice()),
        ] {
            let response =
                futures::executor::block_on(client.get(&format!("http://{address}{path}"), follow))
                    .unwrap();
            assert_eq!(response.status.as_u16(), expected_status);
            assert_eq!(response.body, expected_body);
        }
        worker.join().unwrap();
    }
}
