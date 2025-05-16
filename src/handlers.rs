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

    //POST - request
    if request.starts_with("POST /register") {
        return handle_register(&request, &mut stream);
    } else if request.starts_with("POST /login") {
        return handle_login(&request, &mut stream);
    } else if request.starts_with("POST /save") {
        return handle_save(&request, &mut stream);
    } else if request.starts_with("POST /upload") {
        return handle_upload(&request, &mut stream);
    }

    //GET - request
    match path {
        "/" => serve_file("index.html", &mut stream),
        "/about" => serve_file("about.html", &mut stream),
        "/register" => serve_file("register.html", &mut stream),
        "/files" => list_files(&mut stream), //новый маршрут для отображения файлов
        _=> {
            //возвращаем 404 для неизвестных маршрутов
            let response = not_found_response();
            stream.write_all(response.as_bytes())?;
            stream.flush()?;
            Ok(())
        }
    }
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

fn list_files(stream: &mut TcpStream) -> Result<(), HttpError> {
    let mut files_list = String::from("<table><tr><th>Имя файла</th><th>Размер (байт)</th><th>Изменен</th></tr>");
    // PATH FOLDER static/
    let static_dir = std::path::Path::new("static");

    if static_dir.exists() {
        //read folder
        for entry in fs::read_dir(static_dir)? {
            let entry = entry?;
            let path = entry.path();
            //get metadata (size, time, change time)
            let metadata = entry.metadata()?;
            let file_name = path.strip_prefix("static/").unwrap_or(&path).to_string_lossy();
            //file size in bytes
            let size = metadata.len();
            //time last change
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let modified_secs = modified.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let modified_time = get_formatted_time();

            // Добавляем строку таблицы с именем, размером и временем
            files_list.push_str(&format!(
                "<tr><td><a href=\"/static/{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                file_name, file_name, size, modified_time
            ));
        }
    }
else {
    // Если папка отсутствует, показываем сообщение
    files_list.push_str("<tr><td colspan=\"3\">Папка static отсутствует</td></tr>");
}
    files_list.push_str("</table>");
    // Формируем HTML-страницу
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <title>Файловый менеджер</title>
    <link rel="stylesheet" href="/static/styles.css">
    <style>
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
    </style>
</head>
<body>
    <h1>Файловый менеджер</h1>
    <p><a href="/upload">Загрузить файл</a> | <a href="/">На главную</a></p>
    {}
</body>
</html>"#,
        files_list
    );

    // Формируем HTTP-ответ
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
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
    // Извлекаем boundary из Content-Type для парсинга multipart/form-data
    let boundary = request
        .lines()
        .find(|line| line.starts_with("Content-Type: multipart/form-data"))
        .and_then(|line| line.split("boundary=").nth(1))
        .ok_or_else(|| HttpError::Other("Missing boundary".to_string()))?;

    // Получаем тело запроса
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    // Разделяем тело на части по boundary
    let parts = body.split(&format!("--{}", boundary)).filter(|part| !part.trim().is_empty() && part != "--");

    let mut file_name = String::new();
    let mut file_content = Vec::new();

    // Парсим каждую часть
    for part in parts {
        if part.contains("Content-Disposition: form-data") {
            // Извлекаем имя файла из заголовка filename
            if let Some(name_line) = part.lines().find(|line| line.contains("filename=")) {
                if let Some(name) = name_line.split("filename=\"").nth(1).and_then(|s| s.split("\"").next()) {
                    file_name = name.to_string();
                }
            }
            // Извлекаем содержимое файла (после двойного \r\n\r\n)
            if let Some(content_start) = part.find("\r\n\r\n") {
                let content = &part[content_start + 4..];
                let content_end = content.rfind("\r\n").unwrap_or(content.len());
                file_content = content[..content_end].as_bytes().to_vec();
            }
        }
    }

    // Проверяем, получены ли имя и содержимое
    if file_name.is_empty() || file_content.is_empty() {
        return Err(HttpError::Other("Invalid file upload".to_string()));
    }

    // Создаем папку static/uploads/, если не существует
    fs::create_dir_all("static/uploads")?;
    // Формируем путь для сохранения файла
    let file_path = format!("static/uploads/{}", file_name);
    // Сохраняем файл с помощью std::fs::write
    fs::write(&file_path, &file_content)?;

    // Логируем успешную загрузку
    let log_entry = format!("Uploaded file {} at {}", file_name, get_formatted_time());
    log_to_file(&log_entry)?;

    // Формируем HTML-ответ с подтверждением
    let response_body = format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <title>Успех</title>
    <link rel="stylesheet" href="/static/styles.css">
</head>
<body>
    <h1>Файл {} загружен</h1>
    <p><a href="/files">Посмотреть файлы</a> | <a href="/upload">Загрузить ещё</a> | <a href="/">На главную</a></p>
</body>
</html>"#,
        file_name
    );

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn handle_save(request: &str, stream: &mut TcpStream) -> Result<(), HttpError> {
    // Извлекаем тело запроса
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    // Парсим данные формы (application/x-www-form-urlencoded)
    let form_data = parse_form_data(body);
    let content = form_data.get("content").cloned().unwrap_or_default();
    let filename = "user_content.txt";

    // Сохраняем текст в файл
    fs::write(filename, content.as_bytes())?;

    // Формируем HTML-ответ
    let response_body = r#"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <title>Успех</title>
    <link rel="stylesheet" href="/static/styles.css">
</head>
<body>
    <h1>Данные сохранены</h1>
    <p><a href="/">Вернуться на главную</a></p>
</body>
</html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}


