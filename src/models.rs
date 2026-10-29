use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct User {
    pub id: i64,
    pub email: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Board {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BoardData {
    pub board: Board,
    pub columns: Vec<ColumnData>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ColumnData {
    pub id: i64,
    pub name: String,
    pub position: i32,
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Card {
    pub id: i64,
    pub title: String,
    pub details: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewBoard {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewCard {
    pub title: String,
    pub details: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MoveCard {
    pub column_id: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenameColumn {
    pub name: String,
}
