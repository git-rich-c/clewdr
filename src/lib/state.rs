use colored::Colorize;
use rquest::Client;
use rquest::Url;
use rquest::cookie::Cookie;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::error;
use tracing::info;

use std::str::FromStr;
use std::sync::Arc;

use crate::client::AppendHeaders;
use crate::client::SUPER_CLIENT;
use crate::config::Config;
use crate::config::CookieStatus;
use crate::config::Reason;
use crate::error::ClewdrError;

/// State of current connection
#[derive(Clone)]
pub struct AppState {
    pub req_tx: Sender<oneshot::Sender<Result<CookieStatus, ClewdrError>>>,
    pub ret_tx: Sender<(CookieStatus, Option<Reason>)>,
    pub cookie: Option<CookieStatus>,
    pub config: Arc<Config>,
    pub org_uuid: Option<String>,
    pub conv_uuid: Option<String>,
    pub client: Client,
}

impl AppState {
    /// Create a new AppState instance
    pub fn new(
        config: Config,
        req_tx: Sender<oneshot::Sender<Result<CookieStatus, ClewdrError>>>,
        ret_tx: Sender<(CookieStatus, Option<Reason>)>,
    ) -> Self {
        let client = SUPER_CLIENT.cloned();
        AppState {
            config: Arc::new(config),
            req_tx,
            ret_tx,
            cookie: None,
            org_uuid: None,
            conv_uuid: None,
            client,
        }
    }

    /// request a new cookie from cookie manager
    pub async fn request_cookie(&mut self) -> Result<(), ClewdrError> {
        let (one_tx, one_rx) = oneshot::channel();
        self.req_tx.send(one_tx).await?;
        let res = one_rx.await??;
        self.cookie = Some(res.clone());
        self.store_cookie(res.clone())?;
        println!("Cookie: {}", res.cookie.to_string().green());
        Ok(())
    }

    /// store the cookie in the client
    fn store_cookie(&self, cookie: CookieStatus) -> Result<(), ClewdrError> {
        self.client.set_cookie(
            &Url::from_str(self.config.endpoint().as_str())?,
            Cookie::parse(cookie.cookie.to_string().as_str())?,
        );
        Ok(())
    }

    /// return the cookie to the cookie manager
    pub async fn return_cookie(&mut self, reason: Option<Reason>) {
        // return the cookie to the cookie manager
        if let Some(cookie) = self.cookie.take() {
            self.ret_tx
                .send((cookie, reason))
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to send cookie: {}", e);
                });
        }
    }

    /// Delete current chat conversation
    pub async fn delete_chat(&self) -> Result<(), ClewdrError> {
        let Some(ref org_uuid) = self.org_uuid else {
            return Ok(());
        };
        let Some(ref conv_uuid) = self.conv_uuid else {
            return Ok(());
        };
        // if preserve_chats is true, do not delete chat
        if self.config.preserve_chats {
            return Ok(());
        }
        debug!("Deleting chat: {}", conv_uuid);
        let endpoint = format!(
            "{}/api/organizations/{}/chat_conversations/{}",
            self.config.endpoint(),
            org_uuid,
            conv_uuid
        );
        let proxy = self.config.rquest_proxy.clone();
        let _ = self
            .client
            .delete(endpoint)
            .append_headers("", proxy)
            .send()
            .await?;
        info!("Chat deleted");
        Ok(())
    }
}
