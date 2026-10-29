use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::Json;
use rw_contract::errors::UNKNOWN_ERROR_CODE;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// JSON body extractor that maps every rejection to the Node contract error
/// envelope instead of Axum's plain-text response.
pub struct ContractJson<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for ContractJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let path = req.uri().path().to_string();
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ContractJson(value)),
            Err(rejection) => {
                let status = rejection.status();
                // Nest validates with Zod through a pipe and reports every
                // validation failure as 400, while Axum uses 422 for a body
                // that deserializes into the wrong shape.
                let status = if status == StatusCode::UNPROCESSABLE_ENTITY {
                    StatusCode::BAD_REQUEST
                } else {
                    status
                };
                Err(ApiError {
                    status: status.as_u16(),
                    code: UNKNOWN_ERROR_CODE.to_string(),
                    message: rejection.body_text(),
                    path,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ContractJson;
    use axum::body::Body;
    use axum::extract::{FromRequest, Request};
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct TestBody {
        value: u32,
    }

    fn request(payload: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri("/node/test")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn accepts_valid_body() {
        let ContractJson(body) =
            ContractJson::<TestBody>::from_request(request(r#"{"value":7}"#), &())
                .await
                .unwrap();
        assert_eq!(body.value, 7);
    }

    #[tokio::test]
    async fn maps_wrong_shape_to_bad_request() {
        let error = ContractJson::<TestBody>::from_request(request(r#"{"value":"x"}"#), &())
            .await
            .err()
            .expect("wrong shape must fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST.as_u16());
        assert_eq!(error.code, "E000");
    }

    #[tokio::test]
    async fn maps_invalid_body_to_contract_error() {
        let error = ContractJson::<TestBody>::from_request(request("{not json"), &())
            .await
            .err()
            .expect("invalid body must fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST.as_u16());
        assert_eq!(error.code, "E000");
        assert_eq!(error.path, "/node/test");

        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }
}
