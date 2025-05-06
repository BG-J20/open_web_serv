use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::error::Error;
use std::fs::OpenOptions;         //открывает файл для дозаписи
use std::time::{SystemTime, UNIX_EPOCH}; //получает текущее время
use std::ffi::CStr;
use libc::{time_t, tm};
use rusqlite::{params, Connection, Result};
use sha2::{Digest, Sha256};
use urlencoding::decode;
use std::collections::HashMap;


extern "C" {
    fn localtime(time: *const time_t) -> *mut tm;
}

fn init_db() -> Result<()> {
    let conn = Connection::open("users.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (\
            id INTEGER PRIMARY KEY,\
            username TEXT NOT NULL UNIQUE,\
            password_hash TEXT NOT NULL\
            )",
        [],

    )?;
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>>{

    init_db()?;

    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Listening for connections on port 7878");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream) {
                        eprintln!("Error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                let _ = log_to_file(&format!("Error: {}", e));
            }
        }
    }

    Ok(())
}

fn handle_connection(mut stream: std::net::TcpStream) -> Result<(), Box<dyn Error>> {
    let mut buffer = [0; 1024];
    let bytes_read = stream.read(&mut buffer)?;

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("Request client: {}", request);

    let path = request.lines().next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let client_ip = stream.peer_addr()?.ip();

    let now = get_formatted_time();

    let log_entry = format!(
        "[{}] {} запросил {}\n",
        client_ip,
        path,
        now
    );

    //stream.read(&mut buffer).unwrap();
    //println!("Request: {}", String::from_utf8_lossy(&buffer[..bytes_read]));

    //let response = "HTTP/1.1 200 OK\r\n\r\nHello world";

    //stream.write_all(response.as_bytes())?;
    //stream.flush()?;

    log_to_file(&log_entry)?;

    let filename = match path {
        "/" => "index.html",
        "/about" => "about.html",
        _ => "", //404
    };

    //let response = "HTTP/1.1 200 OK\r\n\r\nHello world";

    let response = if !filename.is_empty() {
        match std::fs::read_to_string(filename) {
            Ok(contents) => format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                contents.len(),
                contents

            ),
            Err(_) => not_found_response()
        }
    } else {
        not_found_response()
    };

    //work with POST requestions
    if request.starts_with("POST /register"){
        return handle_register(&request, &mut stream);
    } else if request.starts_with("POST /login"){
        return handle_login(&request, &mut stream);
    } else if path == "/register" {
        return serve_file("register.html", &mut stream);
    }

    stream.write_all(response.as_bytes())?;
    stream.flush()?;


    Ok(())
}

fn handle_register(request: &str, stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {

    let body = request.split("\r\n\r\n").nth(1).unwrap_or(" ");
    let form_data = parse_form_data(body);
    let username = form_data.get("username").cloned().unwrap_or_default();
    let password = form_data.get("password").cloned().unwrap_or_default();

    let hash = hash_password(&password);
    let conn = Connection::open("users.db")?;

    //проверка существует ли пользователь
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users WHERE username = ?1")?;
    let mut rows = stmt.query(params![username])?;
    if let Some(row) = rows.next()? {
        let count: i64 = row.get(0)?;
        if count > 0 {
            return serve_file("user_exists.html", stream);
        }
    }

    //if not users
    let result = conn.execute(
        "INSERT INTO users (username, password_hash) VALUES (?1, ?2)",
        params![username, hash],
    );

    match result {
        Ok(_) => serve_file("registered.html", stream),
        Err(_) => serve_file("unauthorized.html", stream),
    }

}

fn log_to_file(message: &str) -> Result<(), Box<dyn Error>> {

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("log.txt")?;

    file.write_all(message.as_bytes())?;
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
        "HTTP/1.1 404 NOT FOUND\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}
fn get_formatted_time() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let t: time_t = now as time_t;

    unsafe {
        let tm_ptr: *mut tm = localtime(&t);
        if tm_ptr.is_null() {
            return "[время неизвестно]".to_string();
        }

        let tm = *tm_ptr;

        // tm_mon: 0-11, tm_year: с 1900 года
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

/// Определяет Content-Type по расширению файла
fn get_content_type(path: &str) -> &str {
    if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".txt") {
        "text/plain"
    } else {
        "application/octet-stream" // по умолчанию — бинарные данные
    }
}

fn handle_login(request: &str, stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {

    let body = request.split("\r\n\r\n").nth(1).unwrap_or(" ");
    let from_data = parse_form_data(body);
    let username = from_data.get("username").cloned().unwrap_or_default();
    let password = from_data.get("password").cloned().unwrap_or_default();

    let hash = hash_password(&password);

    let conn = Connection::open("users.db")?;
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users WHERE username = ?1 AND password_hash = ?2")?;
    let mut rows = stmt.query(params![username, hash])?;

    if let Some(row) = rows.next()? {
        let count: i64 = row.get(0)?;
        if count > 0 {
            serve_file("welcome.html", stream)?;
        } else {
            serve_file("unauthorized.html", stream)?;
        }
    }
    Ok(())

}

fn parse_form_data(body: &str) -> HashMap<String, String> {

    let mut data = HashMap::new();

    for pair in body.split("&") {
        let mut split = pair.splitn(2, '=');
        if let (Some(k), Some(v)) = (split.next(), split.next()) {
            let key = decode(k).unwrap_or_default().to_string();
            let val = decode(v).unwrap_or_default().to_string();
            data.insert(key, val);
        }
    }
    data
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn serve_file(filename: &str, stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    match std::fs::read_to_string(filename) {
        Ok(contents) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
                contents.len(),
                contents
            );
            stream.write_all(response.as_bytes())?;
        }
        Err(_) => {
            let response = not_found_response();
            stream.write_all(response.as_bytes())?;
        }
    }
    stream.flush()?;
    Ok(())
}