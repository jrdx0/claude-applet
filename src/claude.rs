use base64::{Engine as _, engine::general_purpose};
use log::{info, trace};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use crate::utils::extract_param_from_url;

/// Constant for the Claude usage URL
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ANTHROPIC_AUTH_URL: &str = "https://claude.ai/oauth/authorize";
const ANTHROPIC_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const ANTHROPIC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const ANTHROPIC_AUTH_SCOPE: &str = "user:profile user:inference user:sessions:claude_code";
const OAUTH_REDIRECT_PORT: u16 = 54545;

/// Constants for Claude API error handler
pub const ANTHROPIC_ERROR_AUTH_EXPIRED: &str = "OAuth token has expired";

// Wrapper for the OAuth credentials of Claude AI.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ClaudeCredentials {
    pub access_token: String,
    pub refresh_token: String,
}

// Error details structure for Claude API error responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ErrorDetails {
    pub error_visibility: String,
}

// Error structure for Claude API error responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApiError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    pub details: ErrorDetails,
}

// Top-level error response from Claude API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClaudeErrorResponse {
    #[serde(rename = "type")]
    pub response_type: String, // "error"
    pub error: ApiError,
    pub request_id: String,
}

// It represents the usage period of an account in detail.
// This struct is used inside the response of the Claude API
// usage endpoint.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UsagePeriod {
    pub utilization: f32,
    pub resets_at: Option<String>,
}

// It is part of the response of the Claude API usage endpoint.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<u64>,
    pub used_credits: Option<u64>,
    pub utilization: Option<f32>,
}

// It is the full response of the Claude API usage endpoint.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClaudeUsageResponse {
    // Information about the usage of the account (Current session on the tray).
    pub five_hour: UsagePeriod,
    // Information about the usage of the account (All models).
    pub seven_day: UsagePeriod,
    pub seven_day_oauth_apps: Option<UsagePeriod>,
    pub seven_day_opus: Option<UsagePeriod>,
    pub seven_day_sonnet: Option<UsagePeriod>,
    pub iguana_necktie: Option<UsagePeriod>,
    pub seven_day_iguana_necktie: Option<UsagePeriod>,
    pub extra_usage: ExtraUsage,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Organization {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Account {
    pub uuid: String,
    pub email_address: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnthropicTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
    pub organization: Organization,
    pub account: Account,
}

pub struct GetUsageError {
    pub message: String,
    pub antropic_error_response: Option<ClaudeErrorResponse>,
}

// Generates a code verifier for OAuth2 authorization.
pub fn generate_code_verifier() -> String {
    let random_bytes: [u8; 32] = rand::random();
    general_purpose::URL_SAFE_NO_PAD.encode(random_bytes)
}

// Generates a state for OAuth2 authorization.
pub fn generate_state() -> String {
    let random_bytes: [u8; 32] = rand::random();
    hex::encode(random_bytes)
}

// Generates a code challenge for OAuth2 authorization.
pub fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let hash = hasher.finalize();

    general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

// Runs a localhost server to wait for the OAuth callback.
pub fn wait_for_oauth_callback(expected_state: &str) -> Result<String, String> {
    let listener = TcpListener::bind(format!("127.0.0.1:{OAUTH_REDIRECT_PORT}"))
        .map_err(|e| format!("failed to bind to port {OAUTH_REDIRECT_PORT}: {e}"))?;

    trace!("oauth callback listening on port {OAUTH_REDIRECT_PORT}");

    // Waiting for a connection
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| format!("failed to accept connection: {e}"))?;

    // Reading a HTTP request
    let mut buffer = [0; 1024];
    stream
        .read(&mut buffer)
        .map_err(|e| format!("failed to read from stream: {e}"))?;

    let request = String::from_utf8_lossy(&buffer);

    let received_state = extract_param_from_url(&request, "state")?;

    if received_state != expected_state {
        return Err("state value is not the same".into());
    }

    let code = extract_param_from_url(&request, "code")?;

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Success</h1></body></html>";

    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("failed to write to stream: {e}"))?;

    Ok(code)
}

// Function to exchange code received from the OAuth server for an access token
async fn exchange_code_for_token(
    code: &str,
    state: &str,
    code_verifier: &str,
) -> Result<AnthropicTokenResponse, String> {
    let client = reqwest::Client::new();

    let redirect_url = format!("http://localhost:{OAUTH_REDIRECT_PORT}/callback");

    let request_body = json!({
        "code": code,
        "state": state,
        "grant_type": "authorization_code",
        "client_id": ANTHROPIC_CLIENT_ID,
        "redirect_uri": redirect_url,
        "code_verifier": code_verifier
    });

    trace!("token exchange request body: {request_body}");

    let response = client
        .post(ANTHROPIC_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("failed to send request: {e}"))?;

    let status = response.status();

    let response_text = response
        .text()
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    trace!("token exchange response (status {status}): {response_text}");

    if !status.is_success() {
        return Err(format!(
            "token exchange failed with status {status}: {response_text}"
        ));
    }

    serde_json::from_str::<AnthropicTokenResponse>(&response_text)
        .map_err(|e| format!("failed to parse token response: {e}"))
}

// Function to login to Claude API. It opens a terminal executing `claude /login`.
// When the user exits claude code execution, the terminal is closed and the
// function tries to get the credentials.
pub async fn open_oauth_login() -> Result<AnthropicTokenResponse, String> {
    info!("starting oauth login flow");

    let state = generate_state();
    let code_verifier = generate_code_verifier();

    let code_challenge = generate_code_challenge(&code_verifier);

    trace!("generated pkce verifier and challenge");

    let redirect_url = format!("http://localhost:{OAUTH_REDIRECT_PORT}/callback");
    let auth_url = format!(
        "{}?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        ANTHROPIC_AUTH_URL,                        // Url
        ANTHROPIC_CLIENT_ID,                       // Claude client ID
        urlencoding::encode(&redirect_url),        // Redirect URL
        urlencoding::encode(ANTHROPIC_AUTH_SCOPE), // Scope
        code_challenge,                            // Code challenge
        state                                      // State
    );

    info!("opening browser for authorization");
    webbrowser::open(&auth_url).map_err(|e| format!("failed to open browser: {e}"))?;

    info!("waiting for oauth callback");
    let auth_code = wait_for_oauth_callback(&state)?;
    info!("received authorization code");

    info!("exchanging authorization code for tokens");
    let token_exchanged = exchange_code_for_token(&auth_code, &state, &code_verifier).await?;
    info!("successfully obtained access token");

    Ok(token_exchanged)
}

// Function to get the usage of the account. It receives the access token and returns the usage response.
pub async fn get_usage(access_token: &str) -> Result<ClaudeUsageResponse, GetUsageError> {
    info!("getting usage user information from {CLAUDE_USAGE_URL}");

    let response = reqwest::Client::new()
        .get(CLAUDE_USAGE_URL)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header("anthropic-beta", "oauth-2025-04-20")
        .header(reqwest::header::USER_AGENT, "claude-code/2.0.61")
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| GetUsageError {
            message: format!("error requesting usage: {e}"),
            antropic_error_response: None,
        })?;

    let status = response.status();
    let response_text = response.text().await.map_err(|e| GetUsageError {
        message: format!("error reading response text: {e}"),
        antropic_error_response: None,
    })?;

    info!("request response (status {status}): {response_text}");

    // Try to parse as success response first
    if let Ok(usage) = serde_json::from_str::<ClaudeUsageResponse>(&response_text) {
        return Ok(usage);
    }

    if let Ok(error_response) = serde_json::from_str::<ClaudeErrorResponse>(&response_text) {
        let antropic_error_response = Some(error_response.clone());

        return Err(GetUsageError {
            message: format!(
                "api error ({}): {} [request_id: {}]",
                error_response.error.error_type,
                error_response.error.message,
                error_response.request_id
            ),
            antropic_error_response,
        });
    }

    Err(GetUsageError {
        message: format!("unexpected api response format: {response_text}"),
        antropic_error_response: None,
    })
}

// Function to get the credentials of the account. By default, the
// credentials are stored in a json file within the $HOME/.claude directory.
pub fn get_local_credentials() -> Result<ClaudeCredentials, String> {
    trace!("getting $HOME environment variable");

    let env_home =
        std::env::var("HOME").map_err(|e| format!("home environment variable not set: {e}"))?;

    trace!("reading credentials file located in {env_home}/.config/claude-tray/credentials.json");

    let credentials =
        fs::read_to_string(format!("{env_home}/.config/claude-tray/credentials.json"))
            .map_err(|e| format!("failed to read credentials file: {e}"))?;

    let credentials: ClaudeCredentials = serde_json::from_str(&credentials)
        .map_err(|e| format!("error getting credentials: {e}"))?;

    info!("credentials found in {env_home}/.config/claude-tray/credentials.json");

    Ok(credentials)
}

// Store the credentials in the file credentials.json
pub fn save_credentials_locally(credentials: &AnthropicTokenResponse) -> Result<(), String> {
    let env_home =
        std::env::var("HOME").map_err(|e| format!("home environment variable not set: {e}"))?;

    let config_dir = PathBuf::from(env_home).join(".config/claude-tray");

    trace!("saving credentials to {}", config_dir.display());

    if !config_dir.exists() {
        info!("credentials file not exists. creating new file");

        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }

    let credentials_json = ClaudeCredentials {
        access_token: credentials.access_token.clone(),
        refresh_token: credentials.refresh_token.clone(),
    };

    let json_fmt = serde_json::to_string_pretty(&credentials_json)
        .map_err(|e| format!("failed to serialize credentials: {e}"))?;

    let credentials_file = config_dir.join("credentials.json");

    fs::write(&credentials_file, json_fmt)
        .map_err(|e| format!("failed to write credentials file: {e}"))?;

    info!("credentials saved successfully");

    Ok(())
}

// Refresh credentials using the provided refresh token
pub async fn refresh_credentials(refresh_token: String) -> Result<AnthropicTokenResponse, String> {
    let response = reqwest::Client::new()
        .post(ANTHROPIC_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": ANTHROPIC_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token
        }))
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {e}"))?;

    let status = response.status();

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Token refresh failed with status {status}: {error_text}"
        ));
    }

    let token_response = response
        .json::<AnthropicTokenResponse>()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    Ok(token_response)
}
