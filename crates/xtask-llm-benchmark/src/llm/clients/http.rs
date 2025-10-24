use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client, Response,
};
use std::time::Duration;

#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    /// Build a client with sane defaults.
    pub fn new() -> Result<Self> {
        let inner = Client::builder()
            .user_agent("xtask-llm-benchmark/0.1")
            .timeout(Duration::from_secs(120))
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
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url} non-success status"))?;

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
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("POST {url} non-success status"))?;

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
