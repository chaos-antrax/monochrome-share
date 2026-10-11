use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Deserialize, Serialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceConfiguration {
    pub ice_servers: Vec<IceServer>,
}

#[derive(Clone)]
pub struct IceProvider {
    client: reqwest::Client,
    cached: Arc<Mutex<Option<(Instant, IceConfiguration)>>>,
}

impl IceProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .build()?,
            cached: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn configuration(&self, config: &Config) -> Result<IceConfiguration> {
        let mut result = IceConfiguration::default();
        if let Some(stun) = &config.stun {
            result.ice_servers.push(IceServer {
                urls: vec![stun.replacen("stun://", "stun:", 1)],
                username: None,
                credential: None,
            });
        }
        if let Some(turn) = &config.turn {
            result.ice_servers.push(from_gstreamer_uri(turn)?);
        }
        if let (Some(key), Some(secret)) = (&config.turn_key_id, &config.turn_api_token) {
            // Admission happens before this method. Cache prevents reconnect storms
            // from repeatedly consuming credential-generation API calls.
            let mut cache = self.cached.lock().await;
            if let Some((created, credentials)) = &*cache
                && created.elapsed() < Duration::from_secs(300)
            {
                result.ice_servers.extend(credentials.ice_servers.clone());
                return Ok(result);
            }
            let response = self.client.post(format!("https://rtc.live.cloudflare.com/v1/turn/keys/{key}/credentials/generate-ice-servers"))
                .bearer_auth(secret).json(&serde_json::json!({"ttl": 14400})).send().await
                .context("TURN credential service is unavailable")?;
            anyhow::ensure!(
                response.status().is_success(),
                "TURN credential service rejected the request (HTTP {})",
                response.status()
            );
            let credentials: IceConfiguration = response
                .json()
                .await
                .context("Invalid TURN credential response")?;
            validate_managed_credentials(&credentials)?;
            result.ice_servers.extend(credentials.ice_servers.clone());
            *cache = Some((Instant::now(), credentials));
        }
        Ok(result)
    }
}

fn validate_managed_credentials(config: &IceConfiguration) -> Result<()> {
    anyhow::ensure!(
        config.ice_servers.iter().any(|server| server
            .username
            .as_ref()
            .is_some_and(|u| !u.is_empty())
            && server.credential.as_ref().is_some_and(|c| !c.is_empty())
            && server
                .urls
                .iter()
                .any(|url| url.starts_with("turn:") || url.starts_with("turns:"))),
        "TURN response contained no usable relay credentials"
    );
    for server in &config.ice_servers {
        anyhow::ensure!(
            server.urls.iter().all(|url| url.starts_with("stun:")
                || url.starts_with("turn:")
                || url.starts_with("turns:")),
            "Unexpected ICE URL scheme"
        );
    }
    Ok(())
}

fn from_gstreamer_uri(uri: &str) -> Result<IceServer> {
    let url = reqwest::Url::parse(uri)?;
    let host = url.host_str().context("TURN server has no hostname")?;
    let port = url
        .port()
        .unwrap_or(if url.scheme() == "turns" { 5349 } else { 3478 });
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.into()
    };
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    let username = percent_encoding::percent_decode_str(url.username())
        .decode_utf8()?
        .into_owned();
    let credential =
        percent_encoding::percent_decode_str(url.password().context("TURN URI needs a password")?)
            .decode_utf8()?
            .into_owned();
    anyhow::ensure!(
        !username.is_empty() && !credential.is_empty(),
        "TURN URI needs username and password"
    );
    Ok(IceServer {
        urls: vec![format!("{}:{host}:{port}{query}", url.scheme())],
        username: Some(username),
        credential: Some(credential),
    })
}

/// Convert browser ICE URLs to the libnice/GStreamer TURN URI format.
pub fn gstreamer_turn_uris(config: &IceConfiguration) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for server in &config.ice_servers {
        for uri in &server.urls {
            let Some((scheme, rest)) = uri.split_once(':') else {
                continue;
            };
            if scheme != "turn" && scheme != "turns" {
                continue;
            }
            let mut url = reqwest::Url::parse(&format!("{scheme}://{rest}"))?;
            url.set_username(
                server
                    .username
                    .as_deref()
                    .context("Missing TURN username")?,
            )
            .map_err(|_| anyhow::anyhow!("Invalid TURN username"))?;
            url.set_password(Some(
                server
                    .credential
                    .as_deref()
                    .context("Missing TURN credential")?,
            ))
            .map_err(|_| anyhow::anyhow!("Invalid TURN credential"))?;
            result.push(url.to_string());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_survive_uri_conversion_without_becoming_part_of_public_urls() {
        let original = IceServer {
            urls: vec!["turn:relay.example.com:3478?transport=udp".into()],
            username: Some("session:123".into()),
            credential: Some("secret/+=".into()),
        };
        let configuration = IceConfiguration {
            ice_servers: vec![original.clone()],
        };
        let uri = gstreamer_turn_uris(&configuration).unwrap().remove(0);
        let converted = from_gstreamer_uri(&uri).unwrap();
        assert_eq!(converted.urls, original.urls);
        assert_eq!(converted.username, original.username);
        assert_eq!(converted.credential, original.credential);
        assert!(!converted.urls[0].contains("secret"));
    }
    #[test]
    fn malformed_relay_response_is_rejected() {
        assert!(validate_managed_credentials(&IceConfiguration::default()).is_err());
        assert!(from_gstreamer_uri("turn://relay.example.com:3478").is_err());
    }
}
