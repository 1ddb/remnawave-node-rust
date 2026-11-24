use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, Algorithm, Validation};
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

pub async fn jwt_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    let Some(token) = token else {
        return ApiError::unauthorized(req.uri()).into_response();
    };

    match decode::<Value>(&token, &state.jwt_key, &state.jwt_validation) {
        Ok(_) => next.run(req).await,
        Err(e) => {
            tracing::error!(
                "Incorrect SECRET_KEY or JWT! Request dropped. URL: {}, error: {}",
                req.uri(),
                e
            );
            ApiError::unauthorized(req.uri()).into_response()
        }
    }
}

pub fn jwt_validation() -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.required_spec_claims.remove("exp");
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.validate_aud = false;
    validation.leeway = 0;
    validation
}

pub async fn internal_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let token = req.uri().query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == "token" {
                Some(percent_encoding::percent_decode_str(value).decode_utf8_lossy().into_owned())
            } else {
                None
            }
        })
    });

    let expected = &state.config.internal_rest_token;
    let authorized = token.as_deref().is_some_and(|t| constant_time_eq(t, expected));
    if !authorized {
        return ApiError::unauthorized(req.uri()).into_response();
    }

    next.run(req).await
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn zstd_body(req: Request, next: Next) -> Response {
    let is_zstd = req
        .headers()
        .get(CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("zstd"));

    if !is_zstd {
        return next.run(req).await;
    }

    let (mut parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, crate::MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(e) => return ApiError::internal(&parts.uri, e.to_string()).into_response(),
    };

    let decoded =
        match tokio::task::spawn_blocking(move || decode_zstd_limited(&bytes, crate::MAX_BODY))
            .await
        {
            Ok(Ok(decoded)) => decoded,
            Ok(Err(e)) => return ApiError::internal(&parts.uri, e).into_response(),
            Err(e) => return ApiError::internal(&parts.uri, e.to_string()).into_response(),
        };

    parts.headers.remove(CONTENT_ENCODING);
    parts.headers.remove(CONTENT_LENGTH);
    let req = Request::from_parts(parts, Body::from(decoded));
    next.run(req).await
}

fn decode_zstd_limited(input: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let decoder = zstd::stream::read::Decoder::new(input).map_err(|e| e.to_string())?;
    let mut limited = decoder.take(limit as u64 + 1);
    let mut decoded = Vec::new();
    limited.read_to_end(&mut decoded).map_err(|e| e.to_string())?;
    if decoded.len() > limit {
        return Err(format!("decompressed body exceeds {limit} bytes"));
    }
    Ok(decoded)
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    headers.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    headers.insert("x-xss-protection", HeaderValue::from_static("0"));
    headers.insert(
        "strict-transport-security",
        HeaderValue::from_static("max-age=15552000; includeSubDomains"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    response
}

#[cfg(test)]
mod tests {
    use super::{decode_zstd_limited, jwt_validation};

    #[test]
    fn jwt_validation_matches_node_defaults() {
        let validation = jwt_validation();
        assert!(validation.validate_exp);
        assert!(validation.validate_nbf);
        assert!(!validation.validate_aud);
        assert_eq!(validation.leeway, 0);
        assert!(!validation.required_spec_claims.contains("exp"));
    }

    #[test]
    fn rejects_decompressed_body_over_limit() {
        let compressed = zstd::encode_all(b"hello" as &[u8], 1).unwrap();
        assert!(decode_zstd_limited(&compressed, 4).is_err());
        assert_eq!(decode_zstd_limited(&compressed, 5).unwrap(), b"hello".to_vec());
    }
}
