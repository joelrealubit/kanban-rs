use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use crate::models::{
    Board, BoardData, Card, ColumnData, Credentials, MoveCard, NewBoard, NewCard, RenameColumn,
    User,
};

const SESSION_SECONDS: i64 = 30 * 24 * 60 * 60;
const DEFAULT_COLUMNS: [&str; 5] = ["Backlog", "Planned", "In progress", "Review", "Done"];
const SAMPLE_CARDS: [(&str, &str, usize); 5] = [
    (
        "Map the first-run experience",
        "Sketch the welcome and sign-in flow.",
        0,
    ),
    (
        "Gather launch requirements",
        "Align on the smallest useful first release.",
        0,
    ),
    (
        "Shape the visual direction",
        "Set typography, spacing, and the core palette.",
        1,
    ),
    (
        "Build the board view",
        "Create a calm, readable five-column workspace.",
        2,
    ),
    (
        "Review the release checklist",
        "Confirm the essentials before sharing the app.",
        3,
    ),
];

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        eprintln!("Kanban server error: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The server could not complete the request.",
        )
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Sign in to continue.")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(error)
    }
}

pub async fn connect(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub fn router(pool: SqlitePool, static_dir: &str) -> Router {
    let spa = ServeDir::new(static_dir)
        .not_found_service(ServeFile::new(format!("{static_dir}/index.html")));

    Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(current_user))
        .route("/api/boards", get(list_boards).post(create_board))
        .route("/api/boards/{id}", get(get_board))
        .route("/api/columns/{id}", patch(rename_column))
        .route("/api/columns/{id}/cards", post(create_card))
        .route("/api/cards/{id}", delete(delete_card))
        .route("/api/cards/{id}/move", patch(move_card))
        .with_state(AppState { pool })
        .fallback_service(spa)
}

async fn register(
    State(state): State<AppState>,
    Json(credentials): Json<Credentials>,
) -> Result<Response, ApiError> {
    let user = create_account(&state.pool, credentials).await?;
    let token = create_session(&state.pool, user.id).await?;
    session_response(user, token)
}

async fn login(
    State(state): State<AppState>,
    Json(credentials): Json<Credentials>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&credentials.email)?;
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, email, password_hash FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;

    let password_hash = PasswordHash::new(&row.2).map_err(ApiError::internal)?;
    if Argon2::default()
        .verify_password(credentials.password.as_bytes(), &password_hash)
        .is_err()
    {
        return Err(ApiError::unauthorized());
    }

    let user = User {
        id: row.0,
        email: row.1,
    };
    let token = create_session(&state.pool, user.id).await?;
    session_response(user, token)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash(&token))
            .execute(&state.pool)
            .await?;
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("kanban_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(response)
}

async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<User>, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let (id, email) =
        sqlx::query_as::<_, (i64, String)>("SELECT id, email FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(User { id, email }))
}

async fn list_boards(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Board>>, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, name FROM boards WHERE user_id = ? ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, name)| Board { id, name })
            .collect(),
    ))
}

async fn create_board(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NewBoard>,
) -> Result<Json<Board>, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let name = validate_text(&input.name, 60, "Board name")?;
    let mut transaction = state.pool.begin().await?;
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO boards (user_id, name) VALUES (?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(&name)
    .fetch_one(&mut *transaction)
    .await?;
    create_columns(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(Json(Board { id, name }))
}

async fn get_board(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<BoardData>, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    Ok(Json(load_board(&state.pool, user_id, id).await?))
}

async fn rename_column(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<RenameColumn>,
) -> Result<StatusCode, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let name = validate_text(&input.name, 40, "Column name")?;
    let result = sqlx::query(
        "UPDATE columns SET name = ? WHERE id = ? AND board_id IN (SELECT id FROM boards WHERE user_id = ?)",
    )
    .bind(name)
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Column not found."));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn create_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(column_id): Path<i64>,
    Json(input): Json<NewCard>,
) -> Result<Json<Card>, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let title = validate_text(&input.title, 120, "Card title")?;
    let details = input.details.trim().to_owned();
    if details.chars().count() > 1000 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Card details must be 1,000 characters or fewer.",
        ));
    }

    let owned = sqlx::query_scalar::<_, i64>(
        "SELECT columns.id FROM columns JOIN boards ON boards.id = columns.board_id WHERE columns.id = ? AND boards.user_id = ?",
    )
    .bind(column_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    if owned.is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Column not found."));
    }

    let position = sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM cards WHERE column_id = ?",
    )
    .bind(column_id)
    .fetch_one(&state.pool)
    .await?;
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO cards (column_id, title, details, position) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(column_id)
    .bind(title.clone())
    .bind(details.clone())
    .bind(position)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(Card { id, title, details }))
}

async fn delete_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    let result = sqlx::query(
        "DELETE FROM cards WHERE id = ? AND column_id IN (SELECT columns.id FROM columns JOIN boards ON boards.id = columns.board_id WHERE boards.user_id = ?)",
    )
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Card not found."));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn move_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<MoveCard>,
) -> Result<StatusCode, ApiError> {
    let user_id = authenticate(&state.pool, &headers).await?;
    move_card_for_user(&state.pool, user_id, id, input.column_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_account(pool: &SqlitePool, credentials: Credentials) -> Result<User, ApiError> {
    let email = normalize_email(&credentials.email)?;
    if !(10..=128).contains(&credentials.password.chars().count()) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Password must be between 10 and 128 characters.",
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(credentials.password.as_bytes(), &salt)
        .map_err(ApiError::internal)?
        .to_string();

    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO users (email, password_hash) VALUES (?, ?) RETURNING id",
    )
    .bind(&email)
    .bind(password_hash)
    .fetch_one(&mut *transaction)
    .await;
    let user_id = match inserted {
        Ok((id,)) => id,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "An account with that email already exists.",
            ));
        }
        Err(error) => return Err(ApiError::from(error)),
    };
    let board_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO boards (user_id, name) VALUES (?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind("My first board")
    .fetch_one(&mut *transaction)
    .await?;
    let columns = create_columns(&mut transaction, board_id).await?;

    for (title, details, column_position) in SAMPLE_CARDS {
        sqlx::query("INSERT INTO cards (column_id, title, details, position) VALUES (?, ?, ?, 0)")
            .bind(columns[column_position])
            .bind(title)
            .bind(details)
            .execute(&mut *transaction)
            .await?;
    }

    transaction.commit().await?;
    Ok(User { id: user_id, email })
}

async fn create_columns(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    board_id: i64,
) -> Result<[i64; 5], ApiError> {
    let mut ids = [0; 5];
    for (position, name) in DEFAULT_COLUMNS.iter().enumerate() {
        ids[position] = sqlx::query_scalar::<_, i64>(
            "INSERT INTO columns (board_id, name, position) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(board_id)
        .bind(name)
        .bind(position as i32)
        .fetch_one(&mut **transaction)
        .await?;
    }
    Ok(ids)
}

async fn create_session(pool: &SqlitePool, user_id: i64) -> Result<String, ApiError> {
    let token = Uuid::new_v4().simple().to_string();
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(ApiError::internal)?
        .as_secs() as i64
        + SESSION_SECONDS;
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(token_hash(&token))
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(token)
}

async fn authenticate(pool: &SqlitePool, headers: &HeaderMap) -> Result<i64, ApiError> {
    let token = session_token(headers).ok_or_else(ApiError::unauthorized)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(ApiError::internal)?
        .as_secs() as i64;
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM sessions WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(token_hash(&token))
    .bind(now)
    .fetch_optional(pool)
    .await?
    .ok_or_else(ApiError::unauthorized)
}

async fn load_board(pool: &SqlitePool, user_id: i64, board_id: i64) -> Result<BoardData, ApiError> {
    let (id, name) = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, name FROM boards WHERE id = ? AND user_id = ?",
    )
    .bind(board_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Board not found."))?;
    let column_rows = sqlx::query_as::<_, (i64, String, i32)>(
        "SELECT id, name, position FROM columns WHERE board_id = ? ORDER BY position",
    )
    .bind(board_id)
    .fetch_all(pool)
    .await?;
    let card_rows = sqlx::query_as::<_, (i64, i64, String, String)>(
        "SELECT cards.id, cards.column_id, cards.title, cards.details FROM cards JOIN columns ON columns.id = cards.column_id WHERE columns.board_id = ? ORDER BY cards.position, cards.id",
    )
    .bind(board_id)
    .fetch_all(pool)
    .await?;
    let mut cards_by_column = std::collections::HashMap::<i64, Vec<Card>>::new();
    for (id, column_id, title, details) in card_rows {
        cards_by_column
            .entry(column_id)
            .or_default()
            .push(Card { id, title, details });
    }

    Ok(BoardData {
        board: Board { id, name },
        columns: column_rows
            .into_iter()
            .map(|(id, name, position)| ColumnData {
                id,
                name,
                position,
                cards: cards_by_column.remove(&id).unwrap_or_default(),
            })
            .collect(),
    })
}

async fn move_card_for_user(
    pool: &SqlitePool,
    user_id: i64,
    card_id: i64,
    destination_id: i64,
) -> Result<(), ApiError> {
    let source = sqlx::query_scalar::<_, i64>(
        "SELECT cards.id FROM cards JOIN columns ON columns.id = cards.column_id JOIN boards ON boards.id = columns.board_id WHERE cards.id = ? AND boards.user_id = ?",
    )
    .bind(card_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let destination = sqlx::query_scalar::<_, i64>(
        "SELECT columns.id FROM columns JOIN boards ON boards.id = columns.board_id WHERE columns.id = ? AND boards.user_id = ?",
    )
    .bind(destination_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    if source.is_none() || destination.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "Card or column not found.",
        ));
    }

    let position = sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM cards WHERE column_id = ?",
    )
    .bind(destination_id)
    .fetch_one(pool)
    .await?;
    sqlx::query("UPDATE cards SET column_id = ?, position = ? WHERE id = ?")
        .bind(destination_id)
        .bind(position)
        .bind(card_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    if email.len() > 254
        || email.chars().any(char::is_whitespace)
        || email.matches('@').count() != 1
        || email.starts_with('@')
        || email.ends_with('@')
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Enter a valid email address.",
        ));
    }
    Ok(email)
}

fn validate_text(value: &str, max_length: usize, label: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_length {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("{label} must be between 1 and {max_length} characters."),
        ));
    }
    Ok(value.to_owned())
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == "kanban_session").then(|| value.to_owned()))
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn session_response(user: User, token: String) -> Result<Response, ApiError> {
    let secure = if std::env::var("KANBAN_COOKIE_SECURE").as_deref() == Ok("true") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "kanban_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_SECONDS}{secure}"
    );
    let mut response = (StatusCode::OK, Json(user)).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(ApiError::internal)?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should connect");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations should run");
        pool
    }

    async fn account(pool: &SqlitePool, email: &str) -> User {
        create_account(
            pool,
            Credentials {
                email: email.to_owned(),
                password: "A secure test password".to_owned(),
            },
        )
        .await
        .expect("test account should be created")
    }

    #[test]
    fn email_normalization_rejects_malformed_addresses() {
        assert_eq!(
            normalize_email("  Person@Example.Test ").expect("valid address should normalize"),
            "person@example.test"
        );
        assert_eq!(
            normalize_email("not-an-email")
                .expect_err("malformed addresses should be rejected")
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn registration_seeds_one_board_with_five_columns() {
        let pool = test_pool().await;
        let user = account(&pool, "first@example.test").await;
        let boards = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM boards WHERE user_id = ?")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("board count should be available");
        let columns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM columns JOIN boards ON boards.id = columns.board_id WHERE boards.user_id = ?",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("column count should be available");
        let cards = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cards JOIN columns ON columns.id = cards.column_id JOIN boards ON boards.id = columns.board_id WHERE boards.user_id = ?",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("card count should be available");

        assert_eq!(boards, 1);
        assert_eq!(columns, 5);
        assert_eq!(cards, SAMPLE_CARDS.len() as i64);
    }

    #[tokio::test]
    async fn registration_hashes_password_and_session_authenticates() {
        let pool = test_pool().await;
        let user = account(&pool, "first@example.test").await;
        let password_hash =
            sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .expect("password hash should be saved");
        assert_ne!(password_hash, "A secure test password");
        let parsed = PasswordHash::new(&password_hash).expect("stored hash should be valid");
        assert!(
            Argon2::default()
                .verify_password(b"A secure test password", &parsed)
                .is_ok()
        );
        assert!(
            Argon2::default()
                .verify_password(b"wrong password", &parsed)
                .is_err()
        );

        let token = create_session(&pool, user.id)
            .await
            .expect("session should be stored");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("kanban_session={token}"))
                .expect("test cookie should be valid"),
        );

        assert_eq!(
            authenticate(&pool, &headers)
                .await
                .expect("session should authenticate"),
            user.id
        );
        let stored_token = sqlx::query_scalar::<_, String>("SELECT token_hash FROM sessions")
            .fetch_one(&pool)
            .await
            .expect("hashed session token should be saved");
        assert_eq!(stored_token, token_hash(&token));
        assert_ne!(stored_token, token);
    }

    #[tokio::test]
    async fn board_access_is_scoped_to_its_owner() {
        let pool = test_pool().await;
        let first = account(&pool, "first@example.test").await;
        let second = account(&pool, "second@example.test").await;
        let board_id = sqlx::query_scalar::<_, i64>("SELECT id FROM boards WHERE user_id = ?")
            .bind(first.id)
            .fetch_one(&pool)
            .await
            .expect("seeded board should exist");

        assert!(load_board(&pool, first.id, board_id).await.is_ok());
        assert_eq!(
            load_board(&pool, second.id, board_id)
                .await
                .expect_err("another account must not read the board")
                .status,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn account_cannot_move_another_accounts_card() {
        let pool = test_pool().await;
        let first = account(&pool, "first@example.test").await;
        let second = account(&pool, "second@example.test").await;
        let (card_id, column_id) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT cards.id, columns.id FROM cards JOIN columns ON columns.id = cards.column_id JOIN boards ON boards.id = columns.board_id WHERE boards.user_id = ? LIMIT 1",
        )
        .bind(first.id)
        .fetch_one(&pool)
        .await
        .expect("seeded card should exist");

        let error = move_card_for_user(&pool, second.id, card_id, column_id)
            .await
            .expect_err("another account must not move the card");
        assert_eq!(error.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn moving_a_card_changes_its_column() {
        let pool = test_pool().await;
        let user = account(&pool, "first@example.test").await;
        let board_id = sqlx::query_scalar::<_, i64>("SELECT id FROM boards WHERE user_id = ?")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("seeded board should exist");
        let board = load_board(&pool, user.id, board_id)
            .await
            .expect("board should load");
        let card = board.columns[2].cards[0].clone();
        let destination_id = board.columns[4].id;

        move_card_for_user(&pool, user.id, card.id, destination_id)
            .await
            .expect("moving card should succeed");
        let updated = load_board(&pool, user.id, board_id)
            .await
            .expect("board should reload");

        assert!(
            updated.columns[4]
                .cards
                .iter()
                .any(|candidate| candidate.id == card.id)
        );
        assert!(
            updated.columns[2]
                .cards
                .iter()
                .all(|candidate| candidate.id != card.id)
        );
    }
}
