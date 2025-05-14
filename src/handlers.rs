use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::error::Error;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::db::{authenticate_user, register_user, user_exists};
use crate::utils::{get_formatted_time, hash_password, log_to_file, parse_form_data};

#[derive(Debug)]
pub enum HttpError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Other(String),
}
/*
#[derive(Debug)]
pub struct HttpError {
    pub message: String,
}

impl From<std::io::Error> for HttpError {
    fn from (e: std::io::Error) -> Self {
        HttpError {message: e.to_string()}
    }
}

impl From<rusqlite::Error> for HttpError {
    fn from (e: rusqlite::Error) -> Self {
        HttpError {message: e.to_string()}
    }
}

impl From<Box<dyn std::error::Error>> for HttpError {
    fn from (e: Box<dyn std::error::Error>) -> Self {
        HttpError {message: e.to_string()}
    }
}
*/
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

fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

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
    } else if path == "/admin" {
        return handle_admin_panel(&mut stream);
    } else if path == "/admin/files" {
        return handle_file_manager(&mut stream);
    } else if request.starts_with("POST /upload") {
        return handle_upload(&request, &mut stream);
    } else if path.starts_with("/files/"){
        let filename = &path["/files/".len()..];
        let filepath = format!("uploads/{}", filename);
        return serve_static(&filepath, &mut stream);
    } else if path == "/upload" {
        return serve_file("upload.html", &mut stream);
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

pub fn handle_admin_panel(stream: &mut TcpStream) -> Result<(), HttpError> {
    //let conn = Connection::open("user.db")?;
    let conn = Connection::open("users.db").map_err(HttpError::from)?;

    let mut stmt = conn.prepare("SELECT id, username FROM users")?;
    let users_iter = stmt.query_map([], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut table_rows = String::new();
    for user in users_iter {
        let (id, username) = user?;
        table_rows.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", id, username));
    }

    let html = format!(r#"
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <title>Админ-панель</title>
            <style>
                table {{ border-collapse: collapse; width: 50%; }}
                th, td {{ border: 1px solid black; padding: 8px; }}
            </style>
        </head>
        <body>
            <h1>Админ-панель</h1>
            <table>
                <tr><th>ID</th><th>Имя пользователя</th></tr>
                {}
            </table>
            <p><a href="/">На главную</a></p>
        </body>
        </html>
    "#, table_rows);

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        html.len(),
        html
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())

}

fn handle_file_manager(stream: &mut TcpStream) -> Result<(), HttpError> {
    let files = fs::read_dir("uploads")?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| {
            let name = e.file_name().into_string().unwrap_or_default();
            format!(r#"<li><a href="/files/{}">{}</a></li>"#, name, name)
        })
        .collect::<Vec<String>>()
        .join("\n");

    let mut html = std::fs::read_to_string("file_manager.html")?;
    html = html.replace("{{FILES}}", &files);

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        html.len(),
        html
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn handle_upload(request: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    if let Some(body_start) = request.find("\r\n\r\n") {
        let body = &request[body_start + 4..];

        // Заглушка: сохраняем всё тело как один файл (небезопасно)
        let filename = format!("uploads/uploaded_{}.bin", get_timestamp());
        std::fs::write(&filename, body.as_bytes())?;
    }

    // Перенаправляем обратно
    let response = "HTTP/1.1 303 See Other\r\nLocation: /admin/files\r\n\r\n";
    stream.write_all(response.as_bytes())?;
    Ok(())
}