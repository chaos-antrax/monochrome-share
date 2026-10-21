use axum::{
    Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header, uri::Authority},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use clap::Parser;
#[cfg(feature = "media")]
use futures_util::{SinkExt, StreamExt};
use monochrome_capture::{config::Config, connectivity::IceProvider, protocol::Signal};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    token: Arc<String>,
    viewer: Arc<Semaphore>,
    shutdown: watch::Receiver<bool>,
    ice_provider: IceProvider,
}

#[derive(Deserialize)]
struct Auth {
    token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let config = Config::parse();
    config.validate()?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    if config.desktop_child {
        let shutdown_tx = shutdown_tx.clone();
        std::thread::spawn(move || {
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
            let _ = shutdown_tx.send(true);
        });
    }
    if !config.viewer_only {
        #[cfg(feature = "media")]
        monochrome_capture::media::initialize(&config)?;
        #[cfg(not(feature = "media"))]
        anyhow::bail!(
            "Capture requires --features media and GStreamer MSVC 1.26+. Use --viewer-only for UI and server development."
        );
    }
    let token = config
        .token
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let address = listener.local_addr()?;
    // Deliberate local-console handoff; do not log tokens in requests or diagnostics.
    println!("Viewer: http://{address}/#token={token}");
    if config.bind.ip().is_unspecified() {
        println!(
            "Replace {} with this host's LAN or Tailscale IP on your phone.",
            config.bind.ip()
        );
    }
    let state = AppState {
        config: Arc::new(config),
        token: Arc::new(token),
        viewer: Arc::new(Semaphore::new(1)),
        shutdown: shutdown_rx.clone(),
        ice_provider: IceProvider::new()?,
    };
    let app = Router::new()
        .route("/", get(viewer_page))
        .route("/viewer.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../viewer/viewer.js")) }))
        .route("/viewer.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], include_str!("../viewer/viewer.css")) }))
        .route("/logo.png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], include_bytes!("../logo.png").as_slice()) }))
        .route("/link-preview-inverted-v1.png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], include_bytes!("../viewer/link-preview-inverted-v1.png").as_slice()) }))
        .route("/connection.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../viewer/connection.js")) }))
        .route("/health", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .layer(axum::middleware::from_fn(|request, next: axum::middleware::Next| async move {
            let mut response = next.run(request).await;
            let headers = response.headers_mut();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert("referrer-policy", "no-referrer".parse().unwrap());
            headers.insert("x-content-type-options", "nosniff".parse().unwrap());
            headers.insert("content-security-policy", "default-src 'self'; connect-src 'self'; media-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
            response
        }))
        .with_state(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut shutdown_rx = shutdown_rx;
            tokio::select! {
                _ = tokio::signal::ctrl_c() => { let _ = shutdown_tx.send(true); }
                _ = shutdown_rx.changed() => {}
            }
        })
        .await?;
    Ok(())
}

fn preview_image_url(headers: &HeaderMap, remote: bool) -> Option<String> {
    // The public-sharing mode uses HTTPS at the tunnel; LAN hosting uses HTTP.
    // Use the requested authority, not untrusted forwarded-host/proto headers.
    // Only a public asset URL is constructed: no request path/query or token.
    let authority = headers
        .get(header::HOST)?
        .to_str()
        .ok()?
        .parse::<Authority>()
        .ok()?;
    if authority.as_str().contains('@') {
        return None;
    }
    let scheme = if remote { "https" } else { "http" };
    Some(format!(
        "{scheme}://{authority}/link-preview-inverted-v1.png"
    ))
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}

async fn viewer_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    let image = preview_image_url(&headers, state.config.remote).ok_or(StatusCode::BAD_REQUEST)?;
    // Do not publish a canonical og:url: shared links must retain their original
    // token fragment, and no secret belongs in crawler-facing metadata.
    Ok(Html(include_str!("../viewer/index.html").replace(
        "__LINK_PREVIEW_IMAGE__",
        &escape_html_attribute(&image),
    )))
}

async fn upgrade(
    State(state): State<AppState>,
    Query(auth): Query<Auth>,
    ws: WebSocketUpgrade,
) -> Response {
    if auth.token.as_deref() != Some(state.token.as_str()) {
        return (StatusCode::UNAUTHORIZED, "Invalid access token").into_response();
    }
    let Ok(permit) = state.viewer.clone().try_acquire_owned() else {
        return (StatusCode::CONFLICT, "A viewer is already connected").into_response();
    };
    ws.max_message_size(128 * 1024)
        .max_frame_size(128 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            if state.config.desktop_child {
                println!("Status: viewer-connected");
            }
            if let Err(error) =
                session(socket, &state.config, state.shutdown, state.ice_provider).await
            {
                tracing::warn!(%error, "Viewer session ended");
            }
            if state.config.desktop_child {
                println!("Status: viewer-disconnected");
            }
        })
        .into_response()
}

async fn send_signal(socket: &mut WebSocket, signal: &Signal) -> anyhow::Result<()> {
    socket
        .send(Message::Text(serde_json::to_string(signal)?.into()))
        .await?;
    Ok(())
}

async fn session(
    mut socket: WebSocket,
    config: &Config,
    mut shutdown: watch::Receiver<bool>,
    ice_provider: IceProvider,
) -> anyhow::Result<()> {
    if *shutdown.borrow() {
        return Ok(());
    }
    if config.viewer_only {
        send_signal(
            &mut socket,
            &Signal::Error {
                message: "Host is in viewer-only mode. Desktop capture is disabled.".into(),
            },
        )
        .await?;
        socket.send(Message::Close(None)).await?;
        return Ok(());
    }
    #[cfg(feature = "media")]
    {
        let ice = tokio::select! {
            _ = shutdown.changed() => return Ok(()),
            result = ice_provider.configuration(config) => match result {
                Ok(ice) => ice,
                Err(error) => {
                    tracing::warn!(%error, "Unable to configure relay");
                    send_signal(&mut socket, &Signal::Error { message: "Remote relay setup failed. Check the host's TURN credentials and internet connection.".into() }).await?;
                    return Ok(());
                }
            }
        };
        send_signal(
            &mut socket,
            &Signal::IceConfig {
                ice_servers: ice.ice_servers.clone(),
            },
        )
        .await?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let media = match monochrome_capture::media::MediaSession::start(config, &ice, tx) {
            Ok(media) => media,
            Err(error) => {
                tracing::error!(%error, "Unable to start capture");
                send_signal(&mut socket, &Signal::Error { message: "Capture failed. Check the host console and run the environment diagnostics.".into() }).await?;
                return Ok(());
            }
        };
        let (mut sink, mut stream) = socket.split();
        let mut bus_tick = tokio::time::interval(std::time::Duration::from_millis(100));
        let negotiation_deadline = tokio::time::sleep(std::time::Duration::from_secs(30));
        tokio::pin!(negotiation_deadline);
        let mut answered = false;
        let mut pending_ice: Vec<(u32, String)> = Vec::new();
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        let mut last_pong = std::time::Instant::now();
        let mut last_keyframe = std::time::Instant::now() - std::time::Duration::from_secs(2);
        loop {
            tokio::select! {
                _ = shutdown.changed() => { break; }
                _ = heartbeat.tick() => {
                    anyhow::ensure!(last_pong.elapsed().as_secs() < 30, "Viewer heartbeat timed out");
                    sink.send(Message::Ping(Default::default())).await?;
                }
                _ = &mut negotiation_deadline, if !answered => {
                    anyhow::bail!("Viewer did not answer within 30 seconds");
                }
                outgoing = rx.recv() => {
                    let Some(signal) = outgoing else { break };
                    let fatal = matches!(signal, Signal::Error { .. });
                    sink.send(Message::Text(serde_json::to_string(&signal)?.into())).await?;
                    if fatal { break; }
                }
                _ = bus_tick.tick() => {
                    if let Err(error) = media.check_bus() {
                        tracing::error!(%error, "Media pipeline failed");
                        sink.send(Message::Text(serde_json::to_string(&Signal::Error { message: "The capture pipeline stopped. Check the host console.".into() })?.into())).await?;
                        break;
                    }
                }
                incoming = stream.next() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            let signal: Signal = serde_json::from_str(&text)?;
                            match signal {
                                Signal::Answer { sdp } if !answered => {
                                    media.answer(&sdp).await?;
                                    answered = true;
                                    for (index, candidate) in pending_ice.drain(..) { media.add_ice(index, &candidate); }
                                }
                                Signal::Ice { candidate, sdp_mline_index } => {
                                    anyhow::ensure!(candidate.len() <= 4096 && sdp_mline_index < 2, "Invalid ICE candidate");
                                    if answered { media.add_ice(sdp_mline_index, &candidate); }
                                    else {
                                        anyhow::ensure!(pending_ice.len() < 64, "Too many pending ICE candidates");
                                        pending_ice.push((sdp_mline_index, candidate));
                                    }
                                }
                                Signal::Keyframe {} if last_keyframe.elapsed().as_secs_f32() >= 1.0 => {
                                    media.request_keyframe();
                                    last_keyframe = std::time::Instant::now();
                                }
                                Signal::Keyframe {} => {}
                                _ => anyhow::bail!("Unexpected signaling message"),
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => sink.send(Message::Pong(payload)).await?,
                        Some(Ok(Message::Pong(_))) => { last_pong = std::time::Instant::now(); }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(error)) => return Err(error.into()),
                        _ => anyhow::bail!("Expected JSON text signaling"),
                    }
                }
            }
        }
        let _ = sink.send(Message::Close(None)).await;
    }
    #[cfg(not(feature = "media"))]
    let _ = (&mut shutdown, ice_provider);
    Ok(())
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn preview_uses_authority_and_configured_scheme_without_forwarded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "share.example:8000".parse().unwrap());
        headers.insert("x-forwarded-host", "untrusted.example".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(
            preview_image_url(&headers, false).as_deref(),
            Some("http://share.example:8000/link-preview-inverted-v1.png")
        );
        assert_eq!(
            preview_image_url(&headers, true).as_deref(),
            Some("https://share.example:8000/link-preview-inverted-v1.png")
        );
        headers.insert(header::HOST, "[::1]:8000".parse().unwrap());
        assert_eq!(
            preview_image_url(&headers, false).as_deref(),
            Some("http://[::1]:8000/link-preview-inverted-v1.png")
        );
    }

    #[test]
    fn preview_rejects_invalid_authorities_and_escapes_html() {
        assert!(preview_image_url(&HeaderMap::new(), false).is_none());
        for host in [
            "user:password@example.com",
            "example.com/path",
            "example.com?token=secret",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, host.parse().unwrap());
            assert!(preview_image_url(&headers, false).is_none());
        }
        assert_eq!(escape_html_attribute("&\"<>'"), "&amp;&quot;&lt;&gt;&#39;");
    }
}
