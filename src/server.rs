use std::net::{TcpListener, TcpStream};
use std::thread;

use crate::handlers::handle_connection;
use crate::utils::log_to_file;

pub fn start_server(listener: TcpListener) -> Result<(), Box<dyn std::error::Error>> {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream) {
                        let error_msg = format!("Connection error: {}", e);
                        eprintln!("{}", error_msg);
                        let _ = log_to_file(&error_msg).map_err(|e| eprintln!("Log error: {}", e));
                    }
                });
            }
            Err(e) => {
                let error_msg = format!("Failed to accept connection: {}", e);
                eprintln!("{}", error_msg);
                let _ = log_to_file(&error_msg).map_err(|e| eprintln!("Log error: {}", e));
            }
        }
    }
    Ok(())
}