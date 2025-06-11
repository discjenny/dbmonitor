use std::sync::Arc;
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
    let connection_string = "host=localhost user=postgres password=postgres dbname=dbmonitor";
    
    println!("connecting to PostgreSQL database...");
    
    // Connect to the database
    let (client, connection) = tokio_postgres::connect(connection_string, NoTls).await?;
    
    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("❌ Database connection error: {}", e);
        }
    });
    
    // Test the connection
    let rows = client
        .query("SELECT version()", &[])
        .await?;
    
    if let Some(row) = rows.first() {
        let version: &str = row.get(0);
        println!("✅ Connected to PostgreSQL: {}", version.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
    }
    
    println!("database connected");
    
    Ok(DbPool {
        client: Arc::new(Mutex::new(client)),
    })
}