use dioxus::prelude::*;

use crate::{
    api,
    models::{Board, BoardData, Card, User},
};

#[derive(Clone, Default)]
struct AppState {
    initialized: bool,
    busy: bool,
    user: Option<User>,
    boards: Vec<Board>,
    board: Option<BoardData>,
    error: Option<String>,
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(AppState::default);

    use_effect(move || {
        spawn(async move {
            match api::current_user().await {
                Ok(user) => load_account(state, user).await,
                Err(error) => state.with_mut(|state| {
                    state.initialized = true;
                    if error != "Sign in to continue." {
                        state.error = Some(error);
                    }
                }),
            }
        });
    });

    let snapshot = state.read().clone();
    rsx! {
        document::Title { "Gather — Kanban" }
        document::Stylesheet { href: asset!("/assets/main.css") }
        document::Link {
            rel: "icon",
            r#type: "image/svg+xml",
            href: asset!("/assets/favicon.svg"),
        }
        if !snapshot.initialized {
            main { class: "boot-screen", "Gather" }
        } else if snapshot.user.is_some() {
            Dashboard { state }
        } else {
            SignIn { state }
        }
    }
}

#[component]
fn SignIn(state: Signal<AppState>) -> Element {
    let mut registering = use_signal(|| false);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let snapshot = state.read().clone();
    let is_registering = registering();

    rsx! {
        main { class: "auth-layout",
            section { class: "auth-story",
                a { class: "brand brand-light", href: "/", span { class: "brand-mark", "g" } "gather" }
                div { class: "story-content",
                    p { class: "eyebrow eyebrow-light", "A CLEARER WAY TO MOVE FORWARD" }
                    h1 { "Make space for your best work." }
                    p { class: "story-copy", "A thoughtful home for the ideas, plans, and small wins that make a project feel possible." }
                    div { class: "story-board", aria_hidden: "true",
                        div { class: "story-board-top", span { "THIS WEEK" } span { class: "tiny-dot" } }
                        div { class: "story-lines",
                            span { class: "story-line line-wide" }
                            span { class: "story-line line-short" }
                            span { class: "story-line line-medium" }
                        }
                        div { class: "story-board-footer",
                            span { class: "story-avatar avatar-one", "A" }
                            span { class: "story-avatar avatar-two", "J" }
                            span { class: "story-avatar avatar-three", "M" }
                            span { "+2" }
                        }
                    }
                }
                p { class: "auth-footnote", "Less noise. More momentum." }
            }
            section { class: "auth-panel",
                div { class: "auth-card",
                    a { class: "brand brand-dark", href: "/", span { class: "brand-mark", "g" } "gather" }
                    p { class: "eyebrow", if is_registering { "YOUR NEXT CHAPTER" } else { "WELCOME BACK" } }
                    h2 { if is_registering { "Create your account" } else { "Sign in to your space" } }
                    p { class: "auth-intro",
                        if is_registering { "Start with a calm, ready-to-use board." }
                        else { "Pick up right where your team left off." }
                    }
                    if let Some(error) = snapshot.error {
                        p { class: "error-message", role: "alert", "{error}" }
                    }
                    form {
                        onsubmit: move |event| {
                            event.prevent_default();
                            let email_value = email().trim().to_owned();
                            let password_value = password();
                            let create_account = registering();
                            state.with_mut(|state| {
                                state.busy = true;
                                state.error = None;
                            });
                            spawn(async move {
                                let result = if create_account {
                                    api::register(email_value, password_value).await
                                } else {
                                    api::login(email_value, password_value).await
                                };
                                match result {
                                    Ok(user) => load_account(state, user).await,
                                    Err(error) => state.with_mut(|state| {
                                        state.busy = false;
                                        state.error = Some(error);
                                    }),
                                }
                            });
                        },
                        label { class: "field-label", r#for: "email", "Email address" }
                        input {
                            id: "email",
                            name: "email",
                            r#type: "email",
                            autocomplete: "email",
                            placeholder: "you@example.com",
                            required: true,
                            value: "{email}",
                            oninput: move |event| email.set(event.value()),
                        }
                        label { class: "field-label", r#for: "password", "Password" }
                        input {
                            id: "password",
                            name: "password",
                            r#type: "password",
                            autocomplete: if is_registering { "new-password" } else { "current-password" },
                            placeholder: if is_registering { "At least 10 characters" } else { "Your password" },
                            minlength: if is_registering { "10" } else { "1" },
                            required: true,
                            value: "{password}",
                            oninput: move |event| password.set(event.value()),
                        }
                        button {
                            class: "button button-primary auth-submit",
                            r#type: "submit",
                            disabled: snapshot.busy,
                            if snapshot.busy { "One moment..." } else if is_registering { "Create account" } else { "Sign in" }
                        }
                    }
                    p { class: "auth-switch",
                        if is_registering { "Already have an account?" } else { "New to Gather?" }
                        button {
                            class: "text-button",
                            r#type: "button",
                            onclick: move |_| {
                                registering.set(!registering());
                                state.with_mut(|state| state.error = None);
                            },
                            if is_registering { "Sign in" } else { "Create an account" }
                        }
                    }
                    if is_registering {
                        p { class: "privacy-note", "Your workspace is private to your account." }
                    }
                }
            }
        }
    }
}

#[component]
fn Dashboard(state: Signal<AppState>) -> Element {
    let mut board_name = use_signal(String::new);
    let mut adding_card_to = use_signal(|| None::<i64>);
    let mut card_title = use_signal(String::new);
    let mut card_details = use_signal(String::new);
    let mut dragging = use_signal(|| None::<i64>);
    let snapshot = state.read().clone();
    let Some(board) = snapshot.board.clone() else {
        return rsx! {
            main { class: "boot-screen",
                if let Some(error) = snapshot.error {
                    p { class: "error-message", role: "alert", "{error}" }
                } else {
                    "Loading your board..."
                }
            }
        };
    };

    rsx! {
        main { class: "workspace",
            aside { class: "sidebar",
                a { class: "brand brand-dark", href: "/", span { class: "brand-mark", "g" } "gather" }
                div { class: "workspace-label", "WORKSPACE" }
                div { class: "sidebar-section-title",
                    span { "Your boards" }
                    span { class: "board-count", "{snapshot.boards.len():02}" }
                }
                nav { class: "board-nav", aria_label: "Your boards",
                    for item in snapshot.boards.iter() {
                        {
                            let board_id = item.id;
                            let selected = board.board.id == item.id;
                            rsx! {
                                button {
                                    class: if selected { "board-link selected" } else { "board-link" },
                                    onclick: move |_| {
                                        spawn(async move { select_board(state, board_id).await; });
                                    },
                                    span { class: "board-link-dot" }
                                    span { class: "board-link-name", "{item.name}" }
                                }
                            }
                        }
                    }
                }
                form { class: "new-board-form",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let name = board_name().trim().to_owned();
                        if name.is_empty() { return; }
                        state.with_mut(|state| {
                            state.busy = true;
                            state.error = None;
                        });
                        spawn(async move {
                            match api::create_board(name).await {
                                Ok(new_board) => {
                                    let id = new_board.id;
                                    state.with_mut(|state| {
                                        state.boards.push(new_board);
                                        state.busy = false;
                                    });
                                    board_name.set(String::new());
                                    select_board(state, id).await;
                                }
                                Err(error) => state.with_mut(|state| {
                                    state.busy = false;
                                    state.error = Some(error);
                                }),
                            }
                        });
                    },
                    input {
                        aria_label: "New board name",
                        placeholder: "Board name",
                        maxlength: "60",
                        value: "{board_name}",
                        oninput: move |event| board_name.set(event.value()),
                    }
                    button { class: "button button-quiet", r#type: "submit", disabled: snapshot.busy, "+ New board" }
                }
                div { class: "sidebar-spacer" }
                div { class: "sidebar-profile",
                    span { class: "profile-avatar", "{avatar_initial(snapshot.user.as_ref().map(|user| user.email.as_str()).unwrap_or(\"G\"))}" }
                    div { class: "profile-copy",
                        strong { "{snapshot.user.as_ref().map(|user| user.email.as_str()).unwrap_or(\"Guest\")}" }
                        span { "Personal workspace" }
                    }
                    button {
                        class: "icon-button logout-button",
                        r#type: "button",
                        aria_label: "Sign out",
                        title: "Sign out",
                        onclick: move |_| {
                            spawn(async move {
                                match api::logout().await {
                                    Ok(()) => state.set(AppState {
                                        initialized: true,
                                        ..AppState::default()
                                    }),
                                    Err(error) => {
                                        state.with_mut(|state| state.error = Some(error))
                                    }
                                }
                            });
                        },
                        "Out"
                    }
                }
            }
            section { class: "board-area",
                header { class: "topbar",
                    div { class: "breadcrumb", span { "Workspace" } span { class: "breadcrumb-separator", "/" } strong { "{board.board.name}" } }
                    div { class: "topbar-right",
                        span { class: "saved-indicator", span { class: "saved-dot" } "Changes save automatically" }
                    }
                }
                div { class: "board-heading",
                    div {
                        p { class: "eyebrow", "YOUR PROJECT, IN FOCUS" }
                        h1 { "{board.board.name}" }
                        p { class: "board-subtitle", "A little progress, every day." }
                    }
                    div { class: "board-summary",
                        span { class: "summary-number", "{board.columns.iter().map(|column| column.cards.len()).sum::<usize>()}" }
                        span { class: "summary-label", "CARDS ON BOARD" }
                    }
                }
                if let Some(error) = snapshot.error {
                    p { class: "error-message workspace-error", role: "alert", "{error}" }
                }
                div { class: "board-scroll",
                    div { class: "kanban-board",
                        for column in board.columns.iter() {
                            {
                                let column_id = column.id;
                                let board_id = board.board.id;
                                let is_adding = adding_card_to() == Some(column.id);
                                rsx! {
                                    section {
                                        class: "kanban-column",
                                        aria_label: "{column.name} column",
                                        ondragover: move |event| event.prevent_default(),
                                        ondrop: move |_| {
                                            if let Some(card_id) = dragging() {
                                                spawn(async move {
                                                    match api::move_card(card_id, column_id).await {
                                                        Ok(()) => refresh_board(state, board_id).await,
                                                        Err(error) => state.with_mut(|state| state.error = Some(error)),
                                                    }
                                                    dragging.set(None);
                                                });
                                            }
                                        },
                                        header { class: "column-header",
                                            div { class: "column-title-wrap",
                                                span { class: "column-index", "0{column.position + 1}" }
                                                input {
                                                    class: "column-title-input",
                                                    value: "{column.name}",
                                                    maxlength: "40",
                                                    aria_label: "Rename {column.name}",
                                                    onchange: move |event| {
                                                        let name = event.value().trim().to_owned();
                                                        if name.is_empty() { return; }
                                                        spawn(async move {
                                                            match api::rename_column(column_id, name).await {
                                                                Ok(()) => refresh_board(state, board_id).await,
                                                                Err(error) => state.with_mut(|state| state.error = Some(error)),
                                                            }
                                                        });
                                                    },
                                                }
                                            }
                                            span { class: "column-count", "{column.cards.len()}" }
                                        }
                                        div { class: "column-cards",
                                            for card in column.cards.iter() {
                                                CardView {
                                                    card: card.clone(),
                                                    columns: board.columns.clone(),
                                                    board_id,
                                                    state,
                                                    dragging,
                                                }
                                            }
                                            if is_adding {
                                                form {
                                                    class: "card-form",
                                                    onsubmit: move |event| {
                                                        event.prevent_default();
                                                        let title = card_title().trim().to_owned();
                                                        let details = card_details().trim().to_owned();
                                                        if title.is_empty() { return; }
                                                        spawn(async move {
                                                            match api::create_card(column_id, title, details).await {
                                                                Ok(_) => {
                                                                    card_title.set(String::new());
                                                                    card_details.set(String::new());
                                                                    adding_card_to.set(None);
                                                                    refresh_board(state, board_id).await;
                                                                }
                                                                Err(error) => state.with_mut(|state| state.error = Some(error)),
                                                            }
                                                        });
                                                    },
                                                    input {
                                                        autofocus: true,
                                                        aria_label: "Card title",
                                                        placeholder: "Card title",
                                                        maxlength: "120",
                                                        required: true,
                                                        value: "{card_title}",
                                                        oninput: move |event| card_title.set(event.value()),
                                                    }
                                                    textarea {
                                                        aria_label: "Card details",
                                                        placeholder: "A few details...",
                                                        maxlength: "1000",
                                                        rows: "3",
                                                        value: "{card_details}",
                                                        oninput: move |event| card_details.set(event.value()),
                                                    }
                                                    div { class: "card-form-actions",
                                                        button { class: "button button-primary button-small", r#type: "submit", "Add card" }
                                                        button {
                                                            class: "text-button cancel-button",
                                                            r#type: "button",
                                                            onclick: move |_| adding_card_to.set(None),
                                                            "Cancel"
                                                        }
                                                    }
                                                }
                                            } else {
                                                button {
                                                    class: "add-card-button",
                                                    r#type: "button",
                                                    onclick: move |_| {
                                                        card_title.set(String::new());
                                                        card_details.set(String::new());
                                                        adding_card_to.set(Some(column_id));
                                                    },
                                                    span { class: "add-card-plus", "+" }
                                                    "Add a card"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                footer { class: "board-footer", "Keep it simple. Keep it moving." }
            }
        }
    }
}

#[component]
fn CardView(
    card: Card,
    columns: Vec<crate::models::ColumnData>,
    board_id: i64,
    state: Signal<AppState>,
    dragging: Signal<Option<i64>>,
) -> Element {
    let card_id = card.id;
    rsx! {
        article {
            class: "task-card",
            draggable: "true",
            ondragstart: move |_| dragging.set(Some(card_id)),
            ondragend: move |_| dragging.set(None),
            h3 { "{card.title}" }
            if !card.details.is_empty() {
                p { class: "task-details", "{card.details}" }
            }
            div { class: "task-card-footer",
                select {
                    class: "move-select",
                    aria_label: "Move {card.title} to another column",
                    onchange: move |event| {
                        if let Ok(column_id) = event.value().parse::<i64>() {
                            spawn(async move {
                                match api::move_card(card_id, column_id).await {
                                    Ok(()) => refresh_board(state, board_id).await,
                                    Err(error) => state.with_mut(|state| state.error = Some(error)),
                                }
                            });
                        }
                    },
                    option { value: "", selected: true, "Move to..." }
                    for column in columns.iter() {
                        option { value: "{column.id}", "{column.name}" }
                    }
                }
                button {
                    class: "delete-card",
                    r#type: "button",
                    aria_label: "Delete {card.title}",
                    title: "Delete card",
                    onclick: move |_| {
                        spawn(async move {
                            match api::delete_card(card_id).await {
                                Ok(()) => refresh_board(state, board_id).await,
                                Err(error) => state.with_mut(|state| state.error = Some(error)),
                            }
                        });
                    },
                    "Delete"
                }
            }
        }
    }
}

async fn load_account(mut state: Signal<AppState>, user: User) {
    match api::boards().await {
        Ok(boards) => {
            let board = match boards.first() {
                Some(board) => api::board(board.id).await.map(Some),
                None => Ok(None),
            };
            state.with_mut(|state| {
                state.initialized = true;
                state.busy = false;
                state.user = Some(user);
                state.boards = boards;
                match board {
                    Ok(board) => {
                        state.board = board;
                        state.error = None;
                    }
                    Err(error) => {
                        state.board = None;
                        state.error = Some(error);
                    }
                }
            });
        }
        Err(error) => state.with_mut(|state| {
            state.initialized = true;
            state.busy = false;
            state.user = Some(user);
            state.error = Some(error);
        }),
    }
}

async fn select_board(mut state: Signal<AppState>, board_id: i64) {
    state.with_mut(|state| {
        state.busy = true;
        state.error = None;
    });
    match api::board(board_id).await {
        Ok(board) => state.with_mut(|state| {
            state.board = Some(board);
            state.busy = false;
        }),
        Err(error) => state.with_mut(|state| {
            state.busy = false;
            state.error = Some(error);
        }),
    }
}

async fn refresh_board(mut state: Signal<AppState>, board_id: i64) {
    match api::board(board_id).await {
        Ok(board) => state.with_mut(|state| {
            state.board = Some(board);
            state.error = None;
        }),
        Err(error) => state.with_mut(|state| state.error = Some(error)),
    }
}

fn avatar_initial(email: &str) -> String {
    email
        .chars()
        .next()
        .unwrap_or('G')
        .to_uppercase()
        .to_string()
}
