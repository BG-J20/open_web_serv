use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::error::Error;
use std::fs::OpenOptions;         //открывает файл для дозаписи
use std::time::{SystemTime, UNIX_EPOCH}; //получает текущее время
use std::ffi::CStr;
use libc::{time_t, tm};

extern "C" {
    fn localtime(time: *const time_t) -> *mut tm;
}

fn main() -> Result<(), Box<dyn Error>>{
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

    stream.write_all(response.as_bytes())?;
    stream.flush()?;


    Ok(())
}

fn not_found_response() -> String {
    let body = "<h1>404 — Страница не найдена</h1>";
    format!(
        "HTTP/1.1 404 NOT FOUND\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn log_to_file(message: &str) -> Result<(), Box<dyn Error>> {

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("log.txt")?;

    file.write_all(message.as_bytes())?;
    Ok(())
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