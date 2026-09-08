//! Bounded publish extraction. Neither errors nor Debug output retain configuration values.
use axum::body::{to_bytes, Bytes};
use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};
use http::{header, StatusCode};
use spacetimedb_client_api_messages::publish::{PublishRequest, CONTENT_TYPE, MAX_MODULE_BYTES, MAX_REQUEST_BYTES};
use std::collections::BTreeMap;

pub struct PublishBody {
    pub program_bytes: Option<Bytes>,
    pub environment: BTreeMap<String, String>,
}

async fn bounded_body(request: Request, limit: usize) -> Result<Bytes, Response> {
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|len| len > limit as u64)
    {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "publish request exceeds size limit").into_response());
    }
    to_bytes(request.into_body(), limit)
        .await
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, "publish request exceeds size limit").into_response())
}

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequest<S> for PublishBody {
    type Rejection = Response;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let envelope = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case(CONTENT_TYPE)
            });
        let bytes = bounded_body(request, if envelope { MAX_REQUEST_BYTES } else { MAX_MODULE_BYTES }).await?;
        if envelope {
            let request = PublishRequest::decode(&bytes)
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid publish request body").into_response())?;
            Ok(Self {
                program_bytes: (!request.module.is_empty()).then_some(request.module.into()),
                environment: request.environment,
            })
        } else {
            // An absent reset body retains the program, but never retains env values.
            Ok(Self {
                program_bytes: (!bytes.is_empty()).then_some(bytes),
                environment: BTreeMap::new(),
            })
        }
    }
}

pub struct ModuleBody(pub Bytes);

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequest<S> for ModuleBody {
    type Rejection = Response;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        bounded_body(request, MAX_MODULE_BYTES).await.map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[tokio::test]
    async fn legacy_envelope_and_empty_reset_have_complete_input_semantics() {
        let legacy = PublishBody::from_request(Request::new(Body::from("module")), &())
            .await
            .unwrap();
        assert_eq!(legacy.program_bytes.unwrap(), "module");
        assert!(legacy.environment.is_empty());
        let empty = PublishBody::from_request(Request::new(Body::empty()), &())
            .await
            .unwrap();
        assert!(empty.program_bytes.is_none());
        assert!(empty.environment.is_empty());
        let input = PublishRequest {
            module: vec![1, 2, 3],
            environment: BTreeMap::from([("TOKEN".into(), "雪\0".into())]),
        };
        let request = Request::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .body(Body::from(input.encode().unwrap()))
            .unwrap();
        let extracted = PublishBody::from_request(request, &()).await.unwrap();
        assert_eq!(extracted.environment, input.environment);
        assert_eq!(extracted.program_bytes.unwrap(), input.module);
        let reset = PublishRequest {
            module: vec![],
            environment: input.environment,
        };
        let request = Request::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .body(Body::from(reset.encode().unwrap()))
            .unwrap();
        let extracted = PublishBody::from_request(request, &()).await.unwrap();
        assert!(extracted.program_bytes.is_none());
        assert_eq!(extracted.environment, reset.environment);
    }

    #[tokio::test]
    async fn streamed_limits_apply_without_global_body_limit_and_errors_are_redacted() {
        let stream = futures::stream::iter([
            Ok::<_, std::io::Error>(Bytes::from_static(b"12345")),
            Ok(Bytes::from_static(b"67890")),
        ]);
        let error = bounded_body(Request::new(Body::from_stream(stream)), 8)
            .await
            .unwrap_err();
        assert_eq!(error.into_response().status(), StatusCode::PAYLOAD_TOO_LARGE);
        let request = Request::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .body(Body::from(r#"{"module":"","environment":{"KEY":["secret-marker"]}}"#))
            .unwrap();
        let error = PublishBody::from_request(request, &())
            .await
            .err()
            .unwrap()
            .into_response();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(error.into_body(), 1024).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("secret-marker"));
    }
}
