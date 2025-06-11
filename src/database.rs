use std::sync::Arc;
use std::env;
use tokio_postgres::{Client, Error, NoTls};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DbPool {
    pub client: Arc<Mutex<Client>>,
}

impl DbPool {
    pub async fn get_client(&self) -> tokio::sync::MutexGuard<'_, Client> {
        self.client.lock().await
    }
}

pub async fn init_db() -> Result<DbPool, Error> {
    // Read environment variables with defaults
    let db_host = env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
    let db_user = env::var("DB_USER").unwrap_or_else(|_| "postgres".to_string());
    let db_password = env::var("DB_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let db_name = env::var("DB_NAME").unwrap_or_else(|_| "dbmonitor".to_string());
    
    let connection_string = format!(
        "host={} user={} password={} dbname={}",
        db_host, db_user, db_password, db_name
    );
    
    println!("connecting to pgsql database at {}...", db_host);
    
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
    
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("database connection error: {}", e);
        }
    });
    
    let rows = client
        .query("SELECT version()", &[])
        .await?;
    
    if let Some(row) = rows.first() {
        let version: &str = row.get(0);
        println!("{} connected", version.split_whitespace().take(2).collect::<Vec<_>>().join(" v"));
    }
        
    Ok(DbPool {
        client: Arc::new(Mutex::new(client)),
    })
}