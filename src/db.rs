use rusqlite::{params, Connection, Result};

pub fn init_db() -> Result<()> {
    let conn = Connection::open("users.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

pub fn user_exists(conn: &Connection, username: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users WHERE username = ?1")?;
    let count: i64 = stmt.query_row(params![username], |row| row.get(0))?;
    Ok(count > 0)
}

pub fn register_user(conn: &Connection, username: &str, password_hash: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO users (username, password_hash) VALUES (?1, ?2)",
        params![username, password_hash],
    )?;
    Ok(())
}

pub fn authenticate_user(conn: &Connection, username: &str, password_hash: &str) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM users WHERE username = ?1 AND password_hash = ?2",
    )?;
    let count: i64 = stmt.query_row(params![username, password_hash], |row| row.get(0))?;
    Ok(count > 0)
}