use clap::{Parser, ValueEnum};
use std::net::SocketAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum AudioMode {
    Desktop,
    Off,
}

#[derive(Clone, Parser)]
#[command(version, about = "Private Windows screen sharing to a single browser")]
pub struct Config {
    /// Use a specific Tailscale IP or LAN IP to accept phone connections.
    #[arg(long, default_value = "127.0.0.1:8000")]
    pub bind: SocketAddr,
    /// Shared secret; generated per launch when omitted. Prefer the environment variable.
    #[arg(long, env = "SCREEN_SHARE_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    #[arg(long, default_value_t = -1, allow_hyphen_values = true)]
    pub monitor: i32,
    /// D3D11 adapter index for WGC; select NVIDIA on hybrid-GPU laptops.
    #[arg(long, default_value_t = -1, allow_hyphen_values = true)]
    pub adapter: i32,
    #[arg(long, default_value_t = 10_000, value_parser = clap::value_parser!(u32).range(1_000..=50_000))]
    pub bitrate_kbps: u32,
    #[arg(long, value_enum, default_value_t = AudioMode::Desktop)]
    pub audio: AudioMode,
    /// GStreamer STUN URI, e.g. stun://stun.example.com:3478.
    #[arg(long)]
    pub stun: Option<String>,
    /// GStreamer TURN URI; prefer environment to keep credentials out of shell history.
    #[arg(long, env = "SCREEN_SHARE_TURN", hide_env_values = true)]
    pub turn: Option<String>,
    /// Cloudflare Realtime TURN key ID. The API token never leaves the host.
    #[arg(long, env = "SCREEN_SHARE_TURN_KEY_ID")]
    pub turn_key_id: Option<String>,
    #[arg(
        long,
        env = "SCREEN_SHARE_TURN_API_TOKEN",
        hide_env_values = true,
        hide = true
    )]
    pub turn_api_token: Option<String>,
    /// Require TURN configuration; intended for public HTTPS-tunnel sharing.
    #[arg(long)]
    pub remote: bool,
    /// Serve the viewer and diagnostics without capturing the desktop.
    #[arg(long)]
    pub viewer_only: bool,
    /// Managed desktop launcher: stop when its stdin is closed or receives a line.
    #[arg(long, hide = true)]
    pub desktop_child: bool,
}

impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.turn_key_id.is_some() == self.turn_api_token.is_some(),
            "Set both SCREEN_SHARE_TURN_KEY_ID and SCREEN_SHARE_TURN_API_TOKEN"
        );
        if let Some(key) = &self.turn_key_id {
            anyhow::ensure!(
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "Invalid TURN key ID"
            );
        }
        if let Some(secret) = &self.turn_api_token {
            anyhow::ensure!(!secret.trim().is_empty(), "TURN API token is empty");
        }
        anyhow::ensure!(
            !self.remote || self.turn.is_some() || self.turn_key_id.is_some(),
            "Remote sharing needs managed TURN credentials or SCREEN_SHARE_TURN"
        );
        anyhow::ensure!(
            self.monitor >= -1,
            "monitor must be -1 (primary) or a nonnegative index"
        );
        anyhow::ensure!(
            self.adapter >= -1,
            "adapter must be -1 (automatic) or a nonnegative index"
        );
        if let Some(token) = &self.token {
            anyhow::ensure!(
                token.len() >= 16,
                "token must contain at least 16 characters"
            );
        }
        if let Some(stun) = &self.stun {
            anyhow::ensure!(stun.starts_with("stun://"), "STUN must use a stun:// URI");
        }
        if let Some(turn) = &self.turn {
            anyhow::ensure!(
                turn.starts_with("turn://") || turn.starts_with("turns://"),
                "TURN must use turn:// or turns://"
            );
        }
        Ok(())
    }
}

/// Raw video caps explicitly require D3D11 memory; never insert a CPU converter.
pub fn video_chain(config: &Config) -> String {
    format!(
        "d3d11screencapturesrc capture-api=wgc monitor-index={} adapter={} show-cursor=true ! \
         video/x-raw(memory:D3D11Memory),framerate=60/1 ! \
         d3d11convert ! video/x-raw(memory:D3D11Memory),format=NV12,width=1920,height=1080 ! \
         queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! \
         {}",
        config.monitor,
        config.adapter,
        encoder_chain(config)
    )
}

/// Set the real NVENC output profile through downstream caps, not SDP rewriting.
/// Safari advertises constrained-baseline but can reject an offer of Main alone.
pub fn encoder_chain(config: &Config) -> String {
    format!(
        "nvd3d11h264enc name=encoder bitrate={} rc-mode=cbr preset=p4 tune=ultra-low-latency \
         cabac=false bframes=0 rc-lookahead=0 zerolatency=true gop-size=300 repeat-sequence-header=true ! \
         video/x-h264,profile=constrained-baseline ! h264parse config-interval=-1",
        config.bitrate_kbps
    )
}

pub fn pipeline_description(config: &Config) -> String {
    let mut result = format!(
        "webrtcbin name=peer bundle-policy=max-bundle latency=0 {} ! \
         rtph264pay name=video_pay pt=96 config-interval=-1 aggregate-mode=zero-latency ! \
         application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000 ! peer.",
        video_chain(config)
    );
    if config.audio == AudioMode::Desktop {
        result.push_str(
            " wasapi2src loopback=true low-latency=true ! \
             queue max-size-time=100000000 max-size-buffers=0 max-size-bytes=0 ! \
             audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2 ! \
             opusenc bitrate=128000 frame-size=20 inband-fec=true packet-loss-percentage=5 ! \
             rtpopuspay pt=97 ! application/x-rtp,media=audio,encoding-name=OPUS,payload=97,clock-rate=48000 ! peer.",
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_invalid_access_and_capture_configuration() {
        let mut config = Config::parse_from(["host", "--token", "short"]);
        assert!(config.validate().is_err());
        config.token = None;
        config.monitor = -2;
        assert!(config.validate().is_err());
        assert!(Config::try_parse_from(["host", "--bitrate-kbps", "0"]).is_err());
    }

    #[test]
    fn audio_off_removes_the_audio_capture_branch() {
        let config = Config::parse_from(["host", "--audio", "off"]);
        assert!(!pipeline_description(&config).contains("wasapi2src"));
    }

    #[test]
    fn safari_profile_is_encoded_without_relabeling_or_reducing_resolution() {
        let config = Config::parse_from(["host"]);
        let video = video_chain(&config);
        assert!(video.contains("profile=constrained-baseline"));
        assert!(video.contains("cabac=false bframes=0"));
        assert!(video.contains("video/x-raw(memory:D3D11Memory),framerate=60/1"));
        assert!(video.contains("format=NV12,width=1920,height=1080"));
        assert!(!video.contains("profile=main"));
        let pipeline = pipeline_description(&config);
        assert!(pipeline.contains("wasapi2src"));
        assert!(pipeline.contains("encoding-name=OPUS"));
        assert!(pipeline.contains("encoding-name=H264"));
        assert!(!pipeline.contains("profile-level-id="));
    }

    #[test]
    fn remote_mode_requires_complete_relay_configuration() {
        let mut config = Config::parse_from(["host"]);
        config.remote = true;
        config.turn = None;
        config.turn_key_id = None;
        config.turn_api_token = None;
        assert!(config.validate().is_err());
        config.turn_key_id = Some("example-key".into());
        assert!(config.validate().is_err());
        config.turn_api_token = Some("local-secret".into());
        assert!(config.validate().is_ok());
    }
}
