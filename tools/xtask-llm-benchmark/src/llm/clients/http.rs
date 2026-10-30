use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client, Response,
};
use std::time::Duration;

#[derive(serde::Serialize)]
pub(crate) struct HttpResponseError {
    pub status: u16,
    pub body: String,
}

impl std::fmt::Display for HttpResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "provider HTTP {} (response body omitted from console)", self.status)
    }
}

impl std::fmt::Debug for HttpResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for HttpResponseError {}

#[cfg(test)]
pub(super) async fn mock_response(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(request).unwrap()
        })
        .await
        .unwrap()
    });
    (base, server)
}

#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    /// Build a client with sane defaults.
    pub fn new() -> Result<Self> {
        let inner = Client::builder()
            .user_agent("xtask-llm-benchmark/0.1")
            .timeout(Duration::from_secs(660)) // longer than any per-model Tokio timeout
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")?;
        Ok(Self { inner })
    }

    /// GET a URL and return the body as UTF-8 text.
    pub async fn get_text(&self, url: &str, headers: &[(HeaderName, HeaderValue)]) -> Result<String> {
        let mut h = HeaderMap::new();
        for (k, v) in headers {
            h.insert(k, v.clone());
        }

        let resp = self
            .inner
            .get(url)
            .headers(h)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.bytes().await.context("read error response body")?;
            return Err(HttpResponseError {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            }
            .into());
        }

        let bytes = resp.bytes().await.context("read GET body")?;
        let txt = String::from_utf8(bytes.to_vec()).context("decode GET body as UTF-8")?;
        Ok(txt)
    }

    /// POST a JSON body and return the response as UTF-8 text.
    pub async fn post_json<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        headers: &[(HeaderName, HeaderValue)],
        body: &T,
    ) -> Result<String> {
        let mut h = HeaderMap::new();
        for (k, v) in headers {
            h.insert(k, v.clone());
        }
        // Ensure content type is set to application/json unless already present.
        h.entry(CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json"));

        let resp = self
            .inner
            .post(url)
            .headers(h)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.bytes().await.context("read error response body")?;
            return Err(HttpResponseError {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            }
            .into());
        }

        let bytes = resp.bytes().await.context("read POST body")?;
        let txt = String::from_utf8(bytes.to_vec()).context("decode POST body as UTF-8")?;
        Ok(txt)
    }

    /// Convenience for "Authorization: Bearer <token>".
    pub fn bearer(token: &str) -> (HeaderName, HeaderValue) {
        (
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).expect("valid bearer header"),
        )
    }

    pub async fn post_json_raw<T: serde::Serialize>(
        &self,
        url: &str,
        headers: &[(HeaderName, HeaderValue)],
        body: &T,
    ) -> Result<Response> {
        let mut hm = HeaderMap::new();
        hm.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        for (k, v) in headers {
            hm.insert(k.clone(), v.clone());
        }
        let req = self.inner.post(url).headers(hm).json(body);
        let resp = req.send().await.context("POST json send")?;
        Ok(resp)
    }
}
