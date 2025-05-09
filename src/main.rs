use std::error::Error;
use std::net::TcpListener;

use crate::db::init_db;
use crate::server::start_server;

mod db;
mod handlers;
mod server;
mod utils;

const HOST: &str = "127.0.0.1";
const PORT: &str = "7878";

fn main() -> Result<(), Box<dyn Error>> {
    init_db()?;
    let addr = format!("{}:{}", HOST, PORT);
    let listener = TcpListener::bind(&addr)?;
    println!("Server running on http://{}", addr);
    start_server(listener)?;
    Ok(())
}