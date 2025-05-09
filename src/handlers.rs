use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use rusqlite::Connection;

use crate::db::{authenticate_user, register_user, user_exists};
use crate::utils::{get_formatted_time, hash_password, log_to_file, parse_form_data};

#[derive(Debug)]
pub enum HttpError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Other(String),
}

impl From<std::io::Error> for HttpError {
    fn from(err: std::io::Error) -> Self {
        HttpError::Io(err)
    }
}

impl From<rusqlite::Error> for HttpError {
    fn from(err: rusqlite::Error) -> Self {
        HttpError::Sqlite(err)
    }
}

impl From<String> for HttpError {
    fn from(err: String) -> Self {
        HttpError::Other(err)
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Io(err) => write!(f, "IO error: {}", err),
            HttpError::Sqlite(err) => write!(f, "SQLite error: {}", err),
            HttpError::Other(err) => write!(f, "Error: {}", err),
        }
    }
}

impl std::error::Error for HttpError {}

pub fn handle_connection(mut stream: TcpStream) -> Result<(), HttpError> {
    let mut buffer = [0; 1024];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let client_ip = stream.peer_addr()?.ip().to_string();
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let log_entry = format!("[{}] {} requested {} at {}", client_ip, path, path, get_formatted_time());
    log_to_file(&log_entry)?;

    if path.starts_with("/static/") {
        let file_path = &path[1..];
        return serve_static(file_path, &mut stream);
    }

    if request.starts_with("POST /register") {
        return handle_register(&request, &mut stream);
    } else if request.starts_with("POST /login") {
        return handle_login(&request, &mut stream);
    }

    let filename = match path {
        "/" => Some("index.html"),
        "/about" => Some("about.html"),
        "/register" => Some("register.html"),
        _ => None,
    };

    if let Some(filename) = filename {
        serve_file(filename, &mut stream)?;
    } else {
        let response = not_found_response();
        stream.write_all(response.as_bytes())?;
        stream.flush()?;
    }

    Ok(())
}

fn serve_static(path: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    match std::fs::read(path) {
        Ok(contents) => {
            let content_type = get_content_type(path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\n\r\n",
                contents.len(),
                content_type
            );
            stream.write_all(response.as_bytes())?;
            stream.write_all(&contents)?;
        }
        Err(e) => {
            eprintln!("Ошибка чтения файла {}: {}", path, e);
            let response = not_found_response();
            stream.write_all(response.as_bytes())?;
        }
    }
    stream.flush()?;
    Ok(())
}

fn serve_file(filename: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    match std::fs::read_to_string(filename) {
        Ok(contents) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
                contents.len(),
                contents
            );
            stream.write_all(response.as_bytes())?;
        }
        Err(e) => {
            eprintln!("Ошибка чтения файла {}: {}", filename, e);
            let response = not_found_response();
            stream.write_all(response.as_bytes())?;
        }
    }
    stream.flush()?;
    Ok(())
}

fn handle_register(request: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    let form_data = parse_form_data(body);
    let username = form_data.get("username").cloned().unwrap_or_default();
    let password = form_data.get("password").cloned().unwrap_or_default();
    let hash = hash_password(&password);

    let conn = Connection::open("users.db")?;

    if user_exists(&conn, &username)? {
        serve_file("user_exists.html", stream)?;
    } else if register_user(&conn, &username, &hash).is_ok() {
        serve_file("registered.html", stream)?;
    } else {
        serve_file("unauthorized.html", stream)?;
    }

    Ok(())
}

fn handle_login(request: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    let form_data = parse_form_data(body);
    let username = form_data.get("username").cloned().unwrap_or_default();
    let password = form_data.get("password").cloned().unwrap_or_default();
    let hash = hash_password(&password);

    let conn = Connection::open("users.db")?;
    if authenticate_user(&conn, &username, &hash)? {
        serve_file("welcome.html", stream)?;
    } else {
        serve_file("unauthorized.html", stream)?;
    }

    Ok(())
}

fn not_found_response() -> String {
    let body = r#"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <title>Страница не найдена</title>
</head>
<body>
    <h1>404 — Страница не найдена</h1>
    <p>Такой страницы не существует. Попробуйте <a href="/">вернуться на главную</a>.</p>
</body>
</html>"#;

    format!(
        "HTTP/1.1 404 NOT FOUND\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        body.len(),
        body
    )
}

fn get_content_type(path: &str) -> &str {
    match path.rsplit('.').next() {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}