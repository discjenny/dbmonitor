use std::env;
use tokio_postgres::NoTls;
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::collections::HashSet;
use std::time::Instant;

pub type DbPool = Pool<PostgresConnectionManager<NoTls>>;

pub async fn init_db() -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    let db_host = env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
    let db_user = env::var("DB_USER").unwrap_or_else(|_| "postgres".to_string());
    let db_password = env::var("DB_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let db_name = env::var("DB_NAME").unwrap_or_else(|_| "dbmonitor".to_string());
    
    let connection_string = format!(
        "host={} user={} password={} dbname={}",
        db_host, db_user, db_password, db_name
    );
    
    println!("connecting to pgsql database at {}...", db_host);
    
    // Create connection manager
    let manager = PostgresConnectionManager::new_from_stringlike(connection_string, NoTls)?;
    
    // Create connection pool with optimized settings
    let pool = Pool::builder()
        .max_size(20)           // Max 20 connections
        .min_idle(Some(2))      // Keep at least 2 idle connections
        .max_lifetime(Some(std::time::Duration::from_secs(3600))) // 1 hour max lifetime
        .idle_timeout(Some(std::time::Duration::from_secs(600)))  // 10 min idle timeout
        .connection_timeout(std::time::Duration::from_secs(30))   // 30s connection timeout
        .build(manager)
        .await?;
    
    // Test the connection and get version
    {
        let conn = pool.get().await?;
        let rows = conn.query("SELECT version()", &[]).await?;
        
        if let Some(row) = rows.first() {
            let version: &str = row.get(0);
            println!("{} connected with {} active connections", 
                version.split_whitespace().take(2).collect::<Vec<_>>().join(" v").to_lowercase(),
                pool.state().connections
            );
        }
    } // Connection is dropped here
        
    Ok(pool)
}

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn run_migrations(db_pool: &DbPool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = db_pool.get().await?;

    println!("determining migrations...");
    let start = Instant::now();

    // fetch versions that were already applied before running the migrations so we can later determine which ones are new
    let pre_rows = conn
        .query("SELECT version FROM refinery_schema_history", &[])
        .await
        .unwrap_or_default();

    let previously_applied: HashSet<i32> = pre_rows
        .iter()
        .map(|row| row.get::<_, i32>("version"))
        .collect();

    embedded::migrations::runner().run_async(&mut *conn).await?;

    // fetch history again to determine which migrations were applied in this run
    let post_rows = conn
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