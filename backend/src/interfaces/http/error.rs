use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::application::error::AppError;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetail<'a>,
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    message: &'a str,
}

impl ApiError {
    pub fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "internal server error".into(),
        }
    }
}

impl ApiError {
    pub fn unauthenticated() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthenticated",
            message: "sign in required".into(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let (status, code, message) = match &error {
            AppError::Domain(_) | AppError::Validation(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.to_string(),
            ),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", error.to_string()),
            // Storage details stay server-side.
            AppError::Storage(_) => {
                return Self::internal();
            }
            AppError::Unauthorized(_) => (
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "sign in required".into(),
            ),
            // Identity provider details stay server-side too.
            AppError::Upstream(_) => (
                StatusCode::BAD_GATEWAY,
                "identity_unavailable",
                "identity provider is unavailable".into(),
            ),
        };
        Self {
            status,
            code,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: &self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}
