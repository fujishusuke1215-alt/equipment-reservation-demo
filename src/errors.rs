use axum::{
    Json,
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: String,
}

#[derive(Serialize)]
struct ProblemDetails {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    code: &'static str,
    detail: String,
}

impl ApiError {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            title,
            detail: detail.into(),
        }
    }

    pub fn bad_request(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, "Bad request", detail)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication required",
            "ログインしてください。",
        )
    }

    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "insufficient_role",
            "Permission denied",
            "この操作には管理者権限が必要です。",
        )
    }

    pub fn not_found(entity: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Resource not found",
            format!("{entity} が見つかりません。"),
        )
    }

    pub fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, "Conflict", detail)
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Validation failed",
            detail,
        )
    }

    pub fn database(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(database_error) = &error {
            match database_error.code().as_deref() {
                Some("23P01") => {
                    return Self::conflict(
                        "reservation_overlap",
                        "選択した期間には、すでに利用中または承認済みの予約があります。",
                    );
                }
                Some("23505") => {
                    return Self::conflict(
                        "duplicate_value",
                        "同じ備品番号がすでに登録されています。",
                    );
                }
                Some("23514") => {
                    return Self::validation("入力内容がデータベースの制約を満たしていません。");
                }
                _ => {}
            }
        }
        tracing::error!(error = %error, "database operation failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            "処理中に問題が発生しました。時間をおいて再度お試しください。",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = ProblemDetails {
            problem_type: "about:blank",
            title: self.title,
            status: status.as_u16(),
            code: self.code,
            detail: self.detail,
        };
        (
            status,
            [("content-type", "application/problem+json")],
            Json(body),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::database(error)
    }
}

pub async fn normalize_error_response(response: Response) -> Response {
    let status = response.status();
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/problem+json"));
    if status.is_success() || is_problem {
        return response;
    }
    let (title, code, detail) = match status {
        StatusCode::NOT_FOUND => (
            "Resource not found",
            "not_found",
            "指定されたリソースが見つかりません。",
        ),
        StatusCode::METHOD_NOT_ALLOWED => (
            "Method not allowed",
            "method_not_allowed",
            "このリソースでは指定されたHTTPメソッドを使えません。",
        ),
        StatusCode::UNAUTHORIZED => (
            "Authentication required",
            "authentication_required",
            "ログインしてください。",
        ),
        StatusCode::FORBIDDEN => (
            "Permission denied",
            "insufficient_role",
            "この操作を実行する権限がありません。",
        ),
        StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST => (
            "Invalid request",
            "invalid_request",
            "リクエスト形式または入力値を確認してください。",
        ),
        _ => (
            "Internal server error",
            "internal_error",
            "処理中に問題が発生しました。時間をおいて再度お試しください。",
        ),
    };
    let body = ProblemDetails {
        problem_type: "about:blank",
        title,
        status: status.as_u16(),
        code,
        detail: detail.to_owned(),
    };
    Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        )
        .body(Body::from(
            serde_json::to_vec(&body).expect("problem details serialize"),
        ))
        .expect("problem response is valid")
}
