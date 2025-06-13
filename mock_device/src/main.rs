use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;


const BASE_URL: &str = "http://127.0.0.1:3010";
const TOKEN_FILE: &str = "token.txt";

#[derive(Serialize)]
struct LogData {
    decibels: f64,
}

#[derive(Deserialize)]
struct AuthResponse {
    token: Option<String>,
}

struct DecibelSimulator {
    t: i32,
    sine_amplitude: f64,
    sine_frequency: f64,
    sine_phase: f64,
    random_offset: f64,
}

impl DecibelSimulator {
    fn new() -> Self {
        let mut rng = rand::rng();
        Self {
            t: 0,
            sine_amplitude: 10.0 + rng.random::<f64>() * 10.0,
            sine_frequency: 20.0 + rng.random::<f64>() * 40.0,
            sine_phase: rng.random::<f64>() * std::f64::consts::PI * 2.0,
            random_offset: 0.0,
        }
    }

    fn randomize_sine_params(&mut self) {
        let mut rng = rand::rng();
        self.sine_amplitude = 10.0 + rng.random::<f64>() * 10.0;
        self.sine_frequency = 20.0 + rng.random::<f64>() * 40.0;
        self.sine_phase = rng.random::<f64>() * std::f64::consts::PI * 2.0;
    }

    fn maybe_randomize_sine(&mut self) {
        if self.t % 200 == 0 {
            self.randomize_sine_params();
        }
    }

    fn get_next_decibels(&mut self) -> f64 {
        self.maybe_randomize_sine();

        // sine wave for smooth periodic fluctuation
        let sine = ((self.t as f64 + self.sine_phase) / self.sine_frequency).sin() * self.sine_amplitude;

        // small random walk for realism, but keep it bounded
        let mut rng = rand::rng();
        self.random_offset += (rng.random::<f64>() - 0.5) * 0.5;
        
        // keep random_offset within -5 to +5
        self.random_offset = self.random_offset.max(-5.0).min(5.0);

        // base value in the middle of the range
        let base = 65.0;

        // calculate next value
        let mut decibels = base + sine + self.random_offset;

        // clamp to 50-80
        decibels = decibels.max(50.0).min(80.0);
        self.t += 1;

        // round to 1 decimal place
        (decibels * 10.0).round() / 10.0
    }
}

async fn fetch_token(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
    let response = client.get(&format!("{}/api/auth", BASE_URL)).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("auth request failed ({})", response.status()).into());
    }

    // check for token in header first
    if let Some(token) = response.headers().get("x-device-token") {
        let token_str = token.to_str()?.to_string();
        fs::write(TOKEN_FILE, &token_str)?;
        println!("obtained new token from header");
        return Ok(token_str);
    }

    // fallback to JSON response
    let auth_response: AuthResponse = response.json().await?;
    if let Some(token) = auth_response.token {
        fs::write(TOKEN_FILE, &token)?;
        println!("obtained new token from JSON");
        Ok(token)
    } else {
        Err("no token in auth response".into())
    }
}

async fn get_token(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
    if Path::new(TOKEN_FILE).exists() {
        if let Ok(token) = fs::read_to_string(TOKEN_FILE) {
            let token = token.trim();
            if !token.is_empty() {
                return Ok(token.to_string());
            }
        }
    }
    fetch_token(client).await
}

async fn post_log(
    client: &Client,
    token: &str,
    simulator: &mut DecibelSimulator,
) -> Result<String, Box<dyn std::error::Error>> {
    let decibels = simulator.get_next_decibels();
    let log_data = LogData { decibels };

    let response = client
        .post(&format!("{}/api/logs", BASE_URL))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .json(&log_data)
        .send()
        .await?;

    if response.status().is_success() {
        println!("sent {{ decibels: {} }}", decibels);
        return Ok(token.to_string());
    }

    if response.status() == 401 {
        println!("token rejected – refreshing");
        let _ = fs::remove_file(TOKEN_FILE);
        let new_token = fetch_token(client).await?;
        return Ok(new_token);
    }

    eprintln!("log post failed ({})", response.status());
    Ok(token.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut simulator = DecibelSimulator::new();
    let mut token = get_token(&client).await?;

    println!("starting mock device...");

    let mut interval = tokio::time::interval(Duration::from_millis(100));
    
    loop {
        interval.tick().await;
        
        match post_log(&client, &token, &mut simulator).await {
            Ok(new_token) => token = new_token,
            Err(err) => eprintln!("unexpected error: {}", err),
        }
    }
} 