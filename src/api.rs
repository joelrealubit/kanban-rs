use crate::models::{
    Board, BoardData, Card, Credentials, MoveCard, NewBoard, NewCard, RenameColumn, User,
};
use serde::de::DeserializeOwned;

#[cfg(not(target_arch = "wasm32"))]
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("the native HTTP client should initialize")
    })
}

#[cfg(target_arch = "wasm32")]
fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[cfg(target_arch = "wasm32")]
fn api_url(path: &str) -> String {
    let origin = web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .unwrap_or_default();
    format!("{origin}{path}")
}

#[cfg(not(target_arch = "wasm32"))]
fn api_url(path: &str) -> String {
    #[cfg(target_os = "android")]
    let origin = option_env!("KANBAN_API_URL")
        .expect("Set KANBAN_API_URL to the HTTPS server origin when building for Android");
    #[cfg(not(target_os = "android"))]
    let origin = option_env!("KANBAN_API_URL").unwrap_or("http://127.0.0.1:8080");
    format!("{}{path}", origin.trim_end_matches('/'))
}

async fn decode<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, String> {
    let response = with_credentials(request)
        .send()
        .await
        .map_err(|error| format!("Could not reach the Kanban server: {error}"))?;

    if !response.status().is_success() {
        return Err(error_message(response).await?);
    }

    response
        .json::<T>()
        .await
        .map_err(|error| format!("The server returned an invalid response: {error}"))
}

async fn send(request: reqwest::RequestBuilder) -> Result<(), String> {
    let response = with_credentials(request)
        .send()
        .await
        .map_err(|error| format!("Could not reach the Kanban server: {error}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(error_message(response).await?)
    }
}

async fn error_message(response: reqwest::Response) -> Result<String, String> {
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        format!("The server failed with status {status}; reading its error failed: {error}")
    })?;
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|body| {
            body.get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    Ok(message.unwrap_or_else(|| format!("The request failed with status {status}.")))
}

#[cfg(target_arch = "wasm32")]
fn with_credentials(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request.fetch_credentials_same_origin()
}

#[cfg(not(target_arch = "wasm32"))]
fn with_credentials(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request
}

pub async fn register(email: String, password: String) -> Result<User, String> {
    decode(
        client()
            .post(api_url("/api/register"))
            .json(&Credentials { email, password }),
    )
    .await
}

pub async fn login(email: String, password: String) -> Result<User, String> {
    decode(
        client()
            .post(api_url("/api/login"))
            .json(&Credentials { email, password }),
    )
    .await
}

pub async fn current_user() -> Result<User, String> {
    decode(client().get(api_url("/api/me"))).await
}

pub async fn logout() -> Result<(), String> {
    send(client().post(api_url("/api/logout"))).await
}

pub async fn boards() -> Result<Vec<Board>, String> {
    decode(client().get(api_url("/api/boards"))).await
}

pub async fn board(id: i64) -> Result<BoardData, String> {
    decode(client().get(api_url(&format!("/api/boards/{id}")))).await
}

pub async fn create_board(name: String) -> Result<Board, String> {
    decode(
        client()
            .post(api_url("/api/boards"))
            .json(&NewBoard { name }),
    )
    .await
}

pub async fn rename_column(id: i64, name: String) -> Result<(), String> {
    send(
        client()
            .patch(api_url(&format!("/api/columns/{id}")))
            .json(&RenameColumn { name }),
    )
    .await
}

pub async fn create_card(column_id: i64, title: String, details: String) -> Result<Card, String> {
    decode(
        client()
            .post(api_url(&format!("/api/columns/{column_id}/cards")))
            .json(&NewCard { title, details }),
    )
    .await
}

pub async fn delete_card(id: i64) -> Result<(), String> {
    send(reqwest::Client::new().delete(api_url(&format!("/api/cards/{id}")))).await
}

pub async fn move_card(id: i64, column_id: i64) -> Result<(), String> {
    send(
        client()
            .patch(api_url(&format!("/api/cards/{id}/move")))
            .json(&MoveCard { column_id }),
    )
    .await
}
