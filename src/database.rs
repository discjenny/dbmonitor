use std::sync::Arc;
use std::env;
use tokio_postgres::{Client, Error, NoTls};
use tokio::sync::Mutex;
use std::collections::HashSet;
use std::time::Instant;

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
        println!("{} connected", version.split_whitespace().take(2).collect::<Vec<_>>().join(" v").to_lowercase());
    }
        
    Ok(DbPool {
        client: Arc::new(Mutex::new(client)),
    })
}

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn run_migrations(db_pool: &DbPool) -> Result<(), refinery::error::Error> {
    let mut client = db_pool.get_client().await;

    println!("determining migrations...");
    let start = Instant::now();

    // fetch versions that were already applied before running the migrations so we can later determine which ones are new
    let pre_rows = client
        .query("SELECT version FROM refinery_schema_history", &[])
        .await
        .unwrap_or_default();

    let previously_applied: HashSet<i32> = pre_rows
        .iter()
        .map(|row| row.get::<_, i32>("version"))
        .collect();

    embedded::migrations::runner().run_async(&mut *client).await?;

    // fetch history again to determine which migrations were applied in this run
    let post_rows = client
        .query(
            "SELECT version, name FROM refinery_schema_history ORDER BY version",
            &[],
        )
        .await
        .unwrap_or_default();

    let mut newly_applied = Vec::new();
    for row in post_rows {
        let version: i32 = row.get("version");
        if !previously_applied.contains(&version) {
            let name: String = row.get("name");
            newly_applied.push((version, name));
        }
    }

    // recompute total migrations based on history rows count
    let total_migrations = previously_applied.len() + newly_applied.len();

    if newly_applied.is_empty() {
        println!(
            "no new migrations found in {:?} ({} already applied, {} total)",
            start.elapsed(),
            previously_applied.len(),
            total_migrations
        );
    } else {
        println!(
            "{} migrations applied in {:?} ({} already applied, {} total)",
            newly_applied.len(),
            start.elapsed(),
            previously_applied.len(),
            total_migrations
        );
        for (ver, name) in newly_applied {
            println!("  • V{}__{}", ver, name);
        }
    }

    Ok(())
}