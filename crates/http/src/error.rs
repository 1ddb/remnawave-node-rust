use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use rw_contract::errors::{self, ErrorDef};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub path: String,
}

#[derive(Serialize)]
struct ErrorBody {
    timestamp: String,
    path: String,
    message: String,
    #[serde(rename = "errorCode")]
    error_code: String,
}

impl ApiError {
    pub fn new(uri: &Uri, def: ErrorDef, message: Option<String>) -> Self {
        Self {
            status: def.http_code,
            code: def.code.to_string(),
            message: message.unwrap_or_else(|| def.message.to_string()),
            path: uri.path().to_string(),
        }
    }

    pub fn internal(uri: &Uri, message: impl Into<String>) -> Self {
        Self::new(uri, errors::INTERNAL_SERVER_ERROR, Some(message.into()))
    }

    pub fn unauthorized(uri: &Uri) -> Self {
        Self {
            status: 401,
            code: errors::UNAUTHORIZED.code.to_string(),
            message: "Unauthorized".into(),
            path: uri.path().to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = ErrorBody {
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            path: self.path,
            message: self.message,
            error_code: self.code,
        };
        (status, Json(body)).into_response()
    }
}
