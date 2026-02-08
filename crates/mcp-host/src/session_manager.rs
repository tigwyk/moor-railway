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

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use eyre::{Result, eyre};
use moor_common::tasks::Event;
use moor_schema::convert::narrative_event_from_ref;
use moor_var::Obj;
use rpc_async_client::pubsub_client::events_recv;
use rpc_async_client::zmq;
use serde_derive::{Deserialize, Serialize};
use tmq::subscribe;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};
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

/// Maximum number of events to buffer per session before dropping oldest
const MAX_EVENT_BUFFER: usize = 1000;

/// Type of narrative event for the buffer
#[derive(Debug, Clone)]
pub enum NarrativeEventType {
    Notify,
    Traceback,
    Present,
    Unpresent,
}

/// A buffered narrative event from the daemon
#[derive(Debug, Clone)]
pub struct BufferedEvent {
    pub event_type: NarrativeEventType,
    pub content: String,
    pub timestamp: SystemTime,
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
    /// Bounded buffer of narrative events from the daemon
    event_buffer: Arc<Mutex<VecDeque<BufferedEvent>>>,
    /// Kill switch for the event subscription background task
    event_kill_switch: Arc<AtomicBool>,
    /// Handle to the background event subscription task
    event_task_handle: Option<JoinHandle<()>>,
}

impl PlayerSession {
    /// Drain and return all buffered events, clearing the buffer.
    pub fn drain_events(&self) -> Vec<BufferedEvent> {
        let mut buffer = self.event_buffer.lock().unwrap();
        buffer.drain(..).collect()
    }

    /// Get the current number of buffered events.
    pub fn event_count(&self) -> usize {
        self.event_buffer.lock().unwrap().len()
    }

    /// Get a clone of the event buffer Arc for async polling.
    pub fn event_buffer_arc(&self) -> Arc<Mutex<VecDeque<BufferedEvent>>> {
        self.event_buffer.clone()
    }

    /// Stop the event subscription background task.
    fn stop_event_subscription(&mut self) {
        self.event_kill_switch.store(true, Ordering::Relaxed);
        if let Some(handle) = self.event_task_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for PlayerSession {
    fn drop(&mut self) {
        self.stop_event_subscription();
    }
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
    /// When true, no service connections are used and all tools require an active session
    pub session_only: bool,
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
        session_only: bool,
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
            session_only,
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

    /// Check if service connections are available (not in session-only mode)
    pub fn has_service_connections(&self) -> bool {
        !self.session_only
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

        // Create the event buffer and kill switch before starting the background task
        let event_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let event_kill_switch = Arc::new(AtomicBool::new(false));

        // Start the background event subscription task
        let event_task_handle = self.start_event_subscription(
            &client,
            session_id,
            event_buffer.clone(),
            event_kill_switch.clone(),
        )?;

        let session = PlayerSession {
            id: session_id,
            username: username.to_string(),
            player: login_info.player,
            created_at: now,
            last_used_at: Mutex::new(now),
            client: Mutex::new(client),
            status: SessionStatus::Active,
            event_buffer,
            event_kill_switch,
            event_task_handle: Some(event_task_handle),
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
        let mut session = self
            .sessions
            .remove(id)
            .ok_or_else(|| eyre!("Session {} not found", id))?;

        // Stop the event subscription task before disconnecting
        session.stop_event_subscription();

        let client = session.client.get_mut().unwrap();
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

    /// Reconnect all established service connections and active/errored sessions.
    ///
    /// Returns a summary of what was reconnected and what failed.
    /// Errored sessions are retried — if they succeed, their status returns to Active.
    pub async fn reconnect_all(&mut self) -> Result<String> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        // Reconnect programmer service
        if let Some(ref mut svc) = self.programmer_service {
            let client = svc.get_mut().unwrap();
            match client.reconnect_with_backoff(3).await {
                Ok(()) => {
                    let player = client
                        .player()
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    succeeded.push(format!("programmer ({})", player));
                }
                Err(e) => {
                    warn!("Failed to reconnect programmer service: {}", e);
                    failed.push(format!("programmer: {}", e));
                }
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
                    succeeded.push(format!("wizard ({})", player));
                }
                Err(e) => {
                    warn!("Failed to reconnect wizard service: {}", e);
                    failed.push(format!("wizard: {}", e));
                }
            }
        }

        // Reconnect active and errored sessions (errored sessions get retried)
        for session in self.sessions.values_mut() {
            if session.status == SessionStatus::Expired {
                continue;
            }
            let client = session.client.get_mut().unwrap();
            match client.reconnect_with_backoff(3).await {
                Ok(()) => {
                    session.status = SessionStatus::Active;
                    succeeded.push(format!("session {} ({})", session.id, session.username));
                }
                Err(e) => {
                    warn!("Failed to reconnect session {}: {}", session.id, e);
                    session.status = SessionStatus::Errored;
                    failed.push(format!("session {} ({}): {}", session.id, session.username, e));
                }
            }
        }

        if succeeded.is_empty() && failed.is_empty() {
            return Ok("No connections to reconnect".to_string());
        }

        let mut parts = Vec::new();
        if !succeeded.is_empty() {
            parts.push(format!("Reconnected: {}", succeeded.join(", ")));
        }
        if !failed.is_empty() {
            parts.push(format!("Failed: {}", failed.join(", ")));
        }
        Ok(parts.join("\n"))
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

    /// Start a background task that subscribes to narrative events for a session's client.
    ///
    /// The task receives events via ZMQ, parses narrative content, and stores it in
    /// the provided bounded buffer. Follows the same ZMQ subscriber pattern as
    /// the ping responder in MoorClient.
    fn start_event_subscription(
        &self,
        client: &MoorClient,
        session_id: Uuid,
        buffer: Arc<Mutex<VecDeque<BufferedEvent>>>,
        kill_switch: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>> {
        let client_id = client.client_id();
        let zmq_context = client.zmq_context().clone();
        let config = client.config().clone();

        // Create events subscriber (same pattern as create_events_subscriber in MoorClient)
        let mut socket_builder = subscribe(&zmq_context);

        if let Some((client_secret, client_public, server_public)) = &config.curve_keys {
            let client_secret_bytes =
                zmq::z85_decode(client_secret).map_err(|_| eyre!("Invalid client secret key"))?;
            let client_public_bytes =
                zmq::z85_decode(client_public).map_err(|_| eyre!("Invalid client public key"))?;
            let server_public_bytes =
                zmq::z85_decode(server_public).map_err(|_| eyre!("Invalid server public key"))?;

            socket_builder = socket_builder
                .set_curve_secretkey(&client_secret_bytes)
                .set_curve_publickey(&client_public_bytes)
                .set_curve_serverkey(&server_public_bytes);
        }

        let events_sub = socket_builder
            .connect(&config.events_address)
            .map_err(|e| eyre!("Unable to connect events subscriber: {}", e))?;

        // Subscribe to this client's events topic
        let mut events_sub = events_sub
            .subscribe(&client_id.as_bytes()[..])
            .map_err(|e| eyre!("Unable to subscribe to client events: {}", e))?;

        let handle = tokio::spawn(async move {
            debug!(
                "Event subscription started for session {} (client {})",
                session_id, client_id
            );

            loop {
                if kill_switch.load(Ordering::Relaxed) {
                    debug!("Event subscription killed for session {}", session_id);
                    break;
                }

                match events_recv(client_id, &mut events_sub).await {
                    Ok(event_msg) => {
                        let Ok(event) = event_msg.event() else {
                            continue;
                        };
                        let Ok(event_union) = event.event() else {
                            continue;
                        };

                        if let Some(buffered) = parse_client_event(event_union) {
                            let mut buf = buffer.lock().unwrap();
                            if buf.len() >= MAX_EVENT_BUFFER {
                                buf.pop_front();
                            }
                            buf.push_back(buffered);
                        }
                    }
                    Err(e) => {
                        if !kill_switch.load(Ordering::Relaxed) {
                            warn!(
                                "Error receiving event for session {}: {:?}",
                                session_id, e
                            );
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }

            debug!("Event subscription stopped for session {}", session_id);
        });

        info!("Event subscription started for session {}", session_id);
        Ok(handle)
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

/// Parse a client event into a BufferedEvent, if it contains narrative content.
fn parse_client_event(
    event_union: moor_schema::rpc::ClientEventUnionRef<'_>,
) -> Option<BufferedEvent> {
    use moor_schema::rpc::ClientEventUnionRef;

    match event_union {
        ClientEventUnionRef::NarrativeEventMessage(narrative_msg) => {
            let event_ref = narrative_msg.event().ok()?;
            let narrative_event = narrative_event_from_ref(event_ref).ok()?;
            let (event_type, content) = match &narrative_event.event {
                Event::Notify { value, .. } => {
                    (NarrativeEventType::Notify, format_var_for_narrative(value))
                }
                Event::Traceback(exception) => (
                    NarrativeEventType::Traceback,
                    format!("** {} **", exception.error),
                ),
                Event::Present(p) => {
                    (NarrativeEventType::Present, format!("{:?}", p))
                }
                Event::Unpresent(id) => {
                    (NarrativeEventType::Unpresent, id.clone())
                }
                Event::SetConnectionOption { .. } => return None,
            };
            trace!("Buffered event: {:?} - {}", event_type, content);
            Some(BufferedEvent {
                event_type,
                content,
                timestamp: narrative_event.timestamp,
            })
        }
        ClientEventUnionRef::SystemMessageEvent(sys_msg) => {
            let msg = sys_msg.message().ok()?;
            Some(BufferedEvent {
                event_type: NarrativeEventType::Notify,
                content: msg.to_string(),
                timestamp: SystemTime::now(),
            })
        }
        _ => None,
    }
}

/// Format a Var for narrative output (mirrors moor_client::format_var_for_narrative)
fn format_var_for_narrative(var: &moor_var::Var) -> String {
    use moor_var::Variant;
    match var.variant() {
        Variant::Str(s) => s.to_string(),
        Variant::Int(i) => i.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Obj(o) => format!("{}", o),
        Variant::List(l) => {
            let items: Vec<String> = l.iter().map(|v| format_var_for_narrative(&v)).collect();
            format!("{{{}}}", items.join(", "))
        }
        Variant::Map(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{} -> {}",
                        format_var_for_narrative(&k),
                        format_var_for_narrative(&v)
                    )
                })
                .collect();
            format!("[{}]", items.join(", "))
        }
        Variant::Err(e) => format!("{}", e),
        Variant::None => "".to_string(),
        Variant::Sym(s) => format!("'{}", s.as_string()),
        Variant::Binary(b) => format!("~<{} bytes>~", b.as_bytes().len()),
        Variant::Lambda(_) => "*lambda*".to_string(),
        Variant::Bool(b) => if b { "true" } else { "false" }.to_string(),
        Variant::Flyweight(f) => format!("{:?}", f),
    }
}
