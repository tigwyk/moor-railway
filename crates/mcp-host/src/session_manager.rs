// Copyright (C) 2026 Ryan Daum <ryan.daum@gmail.com> This program is free
// software: you can redistribute it and/or modify it under the terms of the GNU
// General Public License as published by the Free Software Foundation, version
// 3.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// this program. If not, see <https://www.gnu.org/licenses/>.
//

//! Session manager for multiple dynamic player sessions
//!
//! Manages N concurrent player sessions, each backed by its own `MoorClient`.
//! Service connections (programmer, wizard) are held separately for tool dispatch
//! that doesn't belong to a specific player session.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use eyre::{Result, eyre};
use moor_var::Obj;
use serde_derive::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::moor_client::{LoginInfo, LoginMode, MoorClient, MoorClientConfig};

/// Policy for whether agents can create new player accounts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CreationPolicy {
    /// Anyone can create accounts
    Open,
    /// Creation requires a valid enrollment token
    Token,
    /// Only admin/wizard connections can create accounts
    AdminOnly,
    /// Account creation is disabled entirely
    Disabled,
}

impl std::fmt::Display for CreationPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreationPolicy::Open => write!(f, "open"),
            CreationPolicy::Token => write!(f, "token"),
            CreationPolicy::AdminOnly => write!(f, "admin-only"),
            CreationPolicy::Disabled => write!(f, "disabled"),
        }
    }
}

impl std::str::FromStr for CreationPolicy {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "open" => Ok(CreationPolicy::Open),
            "token" => Ok(CreationPolicy::Token),
            "admin-only" => Ok(CreationPolicy::AdminOnly),
            "disabled" => Ok(CreationPolicy::Disabled),
            _ => Err(format!(
                "invalid creation policy '{s}': expected open, token, admin-only, or disabled"
            )),
        }
    }
}

/// Status of a player session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Errored,
    Expired,
}

/// A single player session backed by a MoorClient connection
pub struct PlayerSession {
    pub id: Uuid,
    pub username: String,
    pub player: Obj,
    pub created_at: SystemTime,
    pub last_used_at: Mutex<SystemTime>,
    pub client: Mutex<MoorClient>,
    pub status: SessionStatus,
}

/// Serializable summary of a session for listing
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: Uuid,
    pub username: String,
    pub player: Obj,
    pub status: SessionStatus,
    pub last_used_at: SystemTime,
}

/// Credentials for a service connection
#[derive(Debug, Clone)]
pub struct ServiceCredentials {
    pub username: String,
    pub password: String,
}

/// Manages dynamic player sessions and static service connections
pub struct SessionManager {
    config: MoorClientConfig,
    programmer_service: Option<Mutex<MoorClient>>,
    wizard_service: Option<Mutex<MoorClient>>,
    programmer_credentials: Option<ServiceCredentials>,
    wizard_credentials: Option<ServiceCredentials>,
    sessions: HashMap<Uuid, PlayerSession>,
    max_sessions: usize,
    session_idle_ttl: Duration,
    creation_policy: CreationPolicy,
    creation_token: Option<String>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(
        config: MoorClientConfig,
        programmer_credentials: Option<ServiceCredentials>,
        wizard_credentials: Option<ServiceCredentials>,
        max_sessions: usize,
        session_idle_ttl: Duration,
        creation_policy: CreationPolicy,
        creation_token: Option<String>,
    ) -> Self {
        Self {
            config,
            programmer_service: None,
            wizard_service: None,
            programmer_credentials,
            wizard_credentials,
            sessions: HashMap::new(),
            max_sessions,
            session_idle_ttl,
            creation_policy,
            creation_token,
        }
    }

    /// Check whether account creation is allowed under the current policy.
    ///
    /// `provided_token` is the token supplied by the caller (if any).
    /// Returns `Ok(())` if creation is allowed, or an error describing why not.
    pub fn check_creation_policy(&self, provided_token: Option<&str>) -> Result<()> {
        match self.creation_policy {
            CreationPolicy::Open => Ok(()),
            CreationPolicy::Token => {
                let expected = self.creation_token.as_deref().ok_or_else(|| {
                    eyre!("Creation policy is 'token' but no creation token is configured on the server")
                })?;
                let provided = provided_token
                    .ok_or_else(|| eyre!("Creation requires an enrollment token"))?;
                if provided == expected {
                    Ok(())
                } else {
                    Err(eyre!("Invalid enrollment token"))
                }
            }
            CreationPolicy::AdminOnly => {
                Err(eyre!("Account creation is restricted to admin connections"))
            }
            CreationPolicy::Disabled => {
                Err(eyre!("Account creation is disabled"))
            }
        }
    }

    /// Get or lazily create the programmer service client (returns Mutex ref)
    pub async fn programmer_service(&mut self) -> Result<&Mutex<MoorClient>> {
        if self.programmer_service.is_none() {
            let creds = self
                .programmer_credentials
                .as_ref()
                .ok_or_else(|| eyre!("No programmer credentials configured"))?;
            let client = self
                .create_and_login(LoginMode::Connect, &creds.username, &creds.password)
                .await?;
            self.programmer_service = Some(Mutex::new(client));
        }
        Ok(self.programmer_service.as_ref().unwrap())
    }

    /// Get or lazily create the wizard service client (returns Mutex ref)
    pub async fn wizard_service(&mut self) -> Result<&Mutex<MoorClient>> {
        if self.wizard_service.is_none() {
            let creds = self
                .wizard_credentials
                .as_ref()
                .ok_or_else(|| eyre!("No wizard credentials configured"))?;
            let client = self
                .create_and_login(LoginMode::Connect, &creds.username, &creds.password)
                .await?;
            self.wizard_service = Some(Mutex::new(client));
        }
        Ok(self.wizard_service.as_ref().unwrap())
    }

    /// Get mutable access to the programmer service client (async, lazily creates)
    pub async fn programmer_service_mut(&mut self) -> Result<&mut MoorClient> {
        if self.programmer_service.is_none() {
            let creds = self
                .programmer_credentials
                .as_ref()
                .ok_or_else(|| eyre!("No programmer credentials configured"))?;
            let client = self
                .create_and_login(LoginMode::Connect, &creds.username, &creds.password)
                .await?;
            self.programmer_service = Some(Mutex::new(client));
        }
        Ok(self
            .programmer_service
            .as_mut()
            .unwrap()
            .get_mut()
            .unwrap())
    }

    /// Get mutable access to the wizard service client (async, lazily creates)
    pub async fn wizard_service_mut(&mut self) -> Result<&mut MoorClient> {
        if self.wizard_service.is_none() {
            let creds = self
                .wizard_credentials
                .as_ref()
                .ok_or_else(|| eyre!("No wizard credentials configured"))?;
            let client = self
                .create_and_login(LoginMode::Connect, &creds.username, &creds.password)
                .await?;
            self.wizard_service = Some(Mutex::new(client));
        }
        Ok(self
            .wizard_service
            .as_mut()
            .unwrap()
            .get_mut()
            .unwrap())
    }

    /// Get mutable access to a session's client by ID
    pub fn get_session_client_mut(&mut self, id: &Uuid) -> Result<&mut MoorClient> {
        let session = self
            .sessions
            .get_mut(id)
            .filter(|s| s.status == SessionStatus::Active)
            .ok_or_else(|| eyre!("Session {} not found or not active", id))?;
        *session.last_used_at.get_mut().unwrap() = SystemTime::now();
        Ok(session.client.get_mut().unwrap())
    }

    /// Check if wizard credentials are configured
    pub fn has_wizard_credentials(&self) -> bool {
        self.wizard_credentials.is_some()
    }

    /// Check if programmer credentials are configured
    pub fn has_programmer_credentials(&self) -> bool {
        self.programmer_credentials.is_some()
    }

    /// Create a new player session
    ///
    /// Connects a new MoorClient, logs in with the given mode and credentials,
    /// and stores the session. Returns the session ID and login info.
    pub async fn create_session(
        &mut self,
        login_mode: LoginMode,
        username: &str,
        password: &str,
    ) -> Result<(Uuid, LoginInfo)> {
        if self.sessions.len() >= self.max_sessions {
            return Err(eyre!(
                "Maximum sessions reached ({}/{})",
                self.sessions.len(),
                self.max_sessions
            ));
        }

        let mut client = MoorClient::new(self.config.clone())?;
        client.connect().await?;
        let login_info = client.login_with_mode(login_mode, username, password).await?;

        let session_id = Uuid::new_v4();
        let now = SystemTime::now();

        let session = PlayerSession {
            id: session_id,
            username: username.to_string(),
            player: login_info.player,
            created_at: now,
            last_used_at: Mutex::new(now),
            client: Mutex::new(client),
            status: SessionStatus::Active,
        };

        info!(
            "Created session {} for {} (player {})",
            session_id, username, login_info.player
        );
        self.sessions.insert(session_id, session);

        Ok((session_id, login_info))
    }

    /// Get a session by ID, if it exists and is active
    pub fn get_session(&self, id: &Uuid) -> Option<&PlayerSession> {
        self.sessions.get(id).filter(|s| s.status == SessionStatus::Active)
    }

    /// Get a mutable session by ID, if it exists and is active
    pub fn get_session_mut(&mut self, id: &Uuid) -> Option<&mut PlayerSession> {
        self.sessions
            .get_mut(id)
            .filter(|s| s.status == SessionStatus::Active)
    }

    /// Close and remove a session
    pub async fn close_session(&mut self, id: &Uuid) -> Result<()> {
        let session = self
            .sessions
            .remove(id)
            .ok_or_else(|| eyre!("Session {} not found", id))?;

        let mut client = session
            .client
            .into_inner()
            .map_err(|_| eyre!("Failed to acquire session client lock"))?;
        if let Err(e) = client.disconnect().await {
            warn!("Error disconnecting session {}: {}", id, e);
        }

        info!("Closed session {} ({})", id, session.username);
        Ok(())
    }

    /// List all sessions with summary info
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .values()
            .map(|s| {
                let last_used = s
                    .last_used_at
                    .lock()
                    .map(|t| *t)
                    .unwrap_or(s.created_at);
                SessionInfo {
                    id: s.id,
                    username: s.username.clone(),
                    player: s.player,
                    status: s.status,
                    last_used_at: last_used,
                }
            })
            .collect()
    }

    /// Remove sessions that have been idle longer than the configured TTL
    pub async fn cleanup_expired_sessions(&mut self) {
        let now = SystemTime::now();
        let ttl = self.session_idle_ttl;

        let expired_ids: Vec<Uuid> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                let last_used = s
                    .last_used_at
                    .lock()
                    .map(|t| *t)
                    .unwrap_or(s.created_at);
                now.duration_since(last_used).unwrap_or(Duration::ZERO) > ttl
            })
            .map(|(id, _)| *id)
            .collect();

        for id in &expired_ids {
            debug!("Expiring idle session {}", id);
            if let Err(e) = self.close_session(id).await {
                warn!("Error closing expired session {}: {}", id, e);
            }
        }

        if !expired_ids.is_empty() {
            info!("Cleaned up {} expired sessions", expired_ids.len());
        }
    }

    /// Reconnect all established service connections and active sessions.
    ///
    /// Returns a summary of what was reconnected.
    pub async fn reconnect_all(&mut self) -> Result<String> {
        let mut results = Vec::new();

        // Reconnect programmer service
        if let Some(ref mut svc) = self.programmer_service {
            let client = svc.get_mut().unwrap();
            match client.reconnect_with_backoff(3).await {
                Ok(()) => {
                    let player = client
                        .player()
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    results.push(format!("programmer ({})", player));
                }
                Err(e) => warn!("Failed to reconnect programmer service: {}", e),
            }
        }

        // Reconnect wizard service
        if let Some(ref mut svc) = self.wizard_service {
            let client = svc.get_mut().unwrap();
            match client.reconnect_with_backoff(3).await {
                Ok(()) => {
                    let player = client
                        .player()
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    results.push(format!("wizard ({})", player));
                }
                Err(e) => warn!("Failed to reconnect wizard service: {}", e),
            }
        }

        // Reconnect active sessions
        for session in self.sessions.values_mut() {
            if session.status != SessionStatus::Active {
                continue;
            }
            let client = session.client.get_mut().unwrap();
            match client.reconnect_with_backoff(3).await {
                Ok(()) => {
                    results.push(format!("session {} ({})", session.id, session.username));
                }
                Err(e) => {
                    warn!("Failed to reconnect session {}: {}", session.id, e);
                    session.status = SessionStatus::Errored;
                }
            }
        }

        if results.is_empty() {
            return Ok("No connections to reconnect".to_string());
        }
        Ok(format!("Reconnected: {}", results.join(", ")))
    }

    /// Gracefully disconnect all service connections and sessions
    pub async fn disconnect_all(&mut self) {
        // Disconnect all player sessions
        let session_ids: Vec<Uuid> = self.sessions.keys().copied().collect();
        for id in &session_ids {
            if let Err(e) = self.close_session(id).await {
                warn!("Error closing session {}: {}", id, e);
            }
        }

        // Disconnect service clients
        if let Some(client_mutex) = self.programmer_service.take()
            && let Ok(mut client) = client_mutex.into_inner()
            && let Err(e) = client.disconnect().await
        {
            warn!("Error disconnecting programmer service: {}", e);
        }
        if let Some(client_mutex) = self.wizard_service.take()
            && let Ok(mut client) = client_mutex.into_inner()
            && let Err(e) = client.disconnect().await
        {
            warn!("Error disconnecting wizard service: {}", e);
        }

        info!("All connections and sessions disconnected");
    }

    /// Create a MoorClient, connect, and log in
    async fn create_and_login(
        &self,
        mode: LoginMode,
        username: &str,
        password: &str,
    ) -> Result<MoorClient> {
        let mut client = MoorClient::new(self.config.clone())?;
        client.connect().await?;
        client.login_with_mode(mode, username, password).await?;
        info!("Service client connected as {}", username);
        Ok(client)
    }
}
