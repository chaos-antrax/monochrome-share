use crate::{
    config::{AudioMode, Config, pipeline_description},
    protocol::Signal,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as webrtc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc::Sender;

pub fn initialize(config: &Config) -> Result<()> {
    anyhow::ensure!(
        cfg!(target_os = "windows"),
        "Desktop capture is supported on Windows only"
    );
    gst::init()?;
    let mut required = vec![
        "d3d11screencapturesrc",
        "d3d11convert",
        "nvd3d11h264enc",
        "h264parse",
        "rtph264pay",
        "webrtcbin",
        "nicesrc",
        "dtlssrtpenc",
    ];
    if config.audio == AudioMode::Desktop {
        required.extend([
            "wasapi2src",
            "audioconvert",
            "audioresample",
            "opusenc",
            "rtpopuspay",
        ]);
    }
    for factory in required {
        anyhow::ensure!(
            gst::ElementFactory::find(factory).is_some(),
            "Missing GStreamer element {factory}; install the MSVC runtime with bad/ugly/libnice plugins and check the NVIDIA driver"
        );
    }
    Ok(())
}

pub struct MediaSession {
    pipeline: gst::Pipeline,
    peer: gst::Element,
    signaling_failed: Arc<AtomicBool>,
}

impl MediaSession {
    pub fn start(
        config: &Config,
        ice: &crate::connectivity::IceConfiguration,
        output: Sender<Signal>,
    ) -> Result<Self> {
        let pipeline = gst::parse::launch(&pipeline_description(config))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Expected GStreamer pipeline"))?;
        let peer = pipeline.by_name("peer").context("Missing webrtcbin")?;
        let signaling_failed = Arc::new(AtomicBool::new(false));
        let session = Self {
            pipeline,
            peer,
            signaling_failed: signaling_failed.clone(),
        };
        if let Some(stun) = ice
            .ice_servers
            .iter()
            .flat_map(|s| &s.urls)
            .find(|uri| uri.starts_with("stun:"))
        {
            session
                .peer
                .set_property("stun-server", stun.replacen("stun:", "stun://", 1));
        }
        for turn in crate::connectivity::gstreamer_turn_uris(ice)? {
            anyhow::ensure!(
                session
                    .peer
                    .emit_by_name::<bool>("add-turn-server", &[&turn]),
                "GStreamer rejected a relay configuration"
            );
        }

        let ice_output = output.clone();
        let ice_failed = signaling_failed.clone();
        session
            .peer
            .connect("on-ice-candidate", false, move |values| {
                let index = values[1].get::<u32>().expect("GStreamer ICE index type");
                let candidate = values[2]
                    .get::<String>()
                    .expect("GStreamer ICE candidate type");
                // Never block GStreamer streaming threads.
                if ice_output
                    .try_send(Signal::Ice {
                        candidate,
                        sdp_mline_index: index,
                    })
                    .is_err()
                {
                    ice_failed.store(true, Ordering::Relaxed);
                    tracing::warn!("Signaling queue full or viewer disconnected");
                }
                None
            });
        let weak_peer = session.peer.downgrade();
        // A single initial offer. This prototype does not renegotiate tracks mid-session.
        let offered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        session
            .peer
            .connect("on-negotiation-needed", false, move |_| {
                if offered.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return None;
                }
                let peer = weak_peer.upgrade()?;
                let weak_peer = peer.downgrade();
                let output = output.clone();
                let signaling_failed = signaling_failed.clone();
                let promise = gst::Promise::with_change_func(move |reply| {
                    let result = (|| -> Result<String> {
                        let reply = reply
                            .map_err(|e| anyhow::anyhow!("Offer promise failed: {e:?}"))?
                            .context("Empty offer promise")?;
                        let offer = reply.get::<webrtc::WebRTCSessionDescription>("offer")?;
                        let peer = weak_peer.upgrade().context("Session closed")?;
                        peer.emit_by_name::<()>(
                            "set-local-description",
                            &[&offer, &None::<gst::Promise>],
                        );
                        Ok(offer.sdp().as_text()?)
                    })();
                    let signal = match result {
                        Ok(sdp) => Signal::Offer { sdp },
                        Err(error) => {
                            tracing::error!(%error, "Offer failed");
                            Signal::Error {
                                message: "Host could not create a WebRTC offer.".into(),
                            }
                        }
                    };
                    if output.try_send(signal).is_err() {
                        signaling_failed.store(true, Ordering::Relaxed);
                    }
                });
                peer.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &promise]);
                None
            });
        session.pipeline.set_state(gst::State::Playing)?;
        Ok(session)
    }

    pub async fn answer(&self, sdp: &str) -> Result<()> {
        let sdp = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes())?;
        let answer = webrtc::WebRTCSessionDescription::new(webrtc::WebRTCSDPType::Answer, sdp);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let promise = gst::Promise::with_change_func(move |reply| {
            let result = (|| -> Result<()> {
                let reply =
                    reply.map_err(|e| anyhow::anyhow!("Remote description failed: {e:?}"))?;
                if let Some(reply) = reply
                    && reply.has_field("error")
                {
                    return Err(reply.get::<gst::glib::Error>("error")?.into());
                }
                Ok(())
            })();
            let _ = tx.send(result);
        });
        self.peer
            .emit_by_name::<()>("set-remote-description", &[&answer, &promise]);
        tokio::time::timeout(std::time::Duration::from_secs(10), rx).await???;
        Ok(())
    }

    pub fn add_ice(&self, index: u32, candidate: &str) {
        self.peer
            .emit_by_name::<()>("add-ice-candidate", &[&index, &candidate]);
    }

    pub fn request_keyframe(&self) {
        if let Some(payloader) = self.pipeline.by_name("video_pay")
            && let Some(pad) = payloader.static_pad("src")
        {
            pad.send_event(
                gstreamer_video::UpstreamForceKeyUnitEvent::builder()
                    .all_headers(true)
                    .build(),
            );
        }
    }

    pub fn check_bus(&self) -> Result<()> {
        anyhow::ensure!(
            !self.signaling_failed.load(Ordering::Relaxed),
            "Signaling delivery failed"
        );
        let bus = self.pipeline.bus().context("Missing pipeline bus")?;
        for message in bus.iter_timed(gst::ClockTime::ZERO) {
            match message.view() {
                gst::MessageView::Error(error) => anyhow::bail!(
                    "{}: {} ({:?})",
                    error.src().map(|s| s.path_string()).unwrap_or_default(),
                    error.error(),
                    error.debug()
                ),
                gst::MessageView::Eos(_) => anyhow::bail!("Capture reached end of stream"),
                gst::MessageView::Warning(warning) => {
                    tracing::warn!(message = %warning.error(), "Media warning")
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        if let Err(error) = self.pipeline.set_state(gst::State::Null) {
            tracing::warn!(?error, "Failed to stop media pipeline");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    #[ignore = "Requires a Windows NVIDIA GPU and GStreamer; uses synthetic frames, never desktop capture"]
    fn nvenc_outputs_real_constrained_baseline_and_matching_rtp_caps() {
        gst::init().unwrap();
        let config = Config::parse_from(["host"]);
        let description = format!(
            "videotestsrc num-buffers=60 pattern=black ! \
             video/x-raw,format=NV12,width=1920,height=1080,framerate=60/1 ! \
             d3d11upload ! video/x-raw(memory:D3D11Memory) ! \
             {} ! rtph264pay name=profile_pay pt=96 config-interval=-1 aggregate-mode=zero-latency ! \
             application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000 ! fakesink sync=false",
            crate::config::encoder_chain(&config)
        );
        let pipeline = gst::parse::launch(&description)
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        let result = (|| -> Result<()> {
            pipeline.set_state(gst::State::Playing)?;
            let bus = pipeline.bus().context("Missing test bus")?;
            let message = bus
                .timed_pop_filtered(
                    gst::ClockTime::from_seconds(30),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                )
                .context("Synthetic NVENC test timed out")?;
            if let gst::MessageView::Error(error) = message.view() {
                anyhow::bail!("NVENC test failed: {} ({:?})", error.error(), error.debug());
            }
            let encoded = pipeline
                .by_name("encoder")
                .unwrap()
                .static_pad("src")
                .unwrap()
                .current_caps()
                .context("Encoder produced no caps")?;
            let caps = encoded.structure(0).unwrap();
            anyhow::ensure!(caps.get::<String>("profile")? == "constrained-baseline");
            anyhow::ensure!(caps.get::<i32>("width")? == 1920);
            anyhow::ensure!(caps.get::<i32>("height")? == 1080);
            anyhow::ensure!(caps.get::<gst::Fraction>("framerate")? == gst::Fraction::new(60, 1));
            let rtp = pipeline
                .by_name("profile_pay")
                .unwrap()
                .static_pad("src")
                .unwrap()
                .current_caps()
                .context("Payloader produced no caps")?;
            let rtp_caps = rtp.structure(0).unwrap();
            let profile = rtp_caps.get::<String>("profile-level-id")?;
            // RFC 6184 constrained-baseline: profile_idc=0x42, constraint_set1=1.
            anyhow::ensure!(profile.len() == 6 && profile.starts_with("42"));
            anyhow::ensure!(u8::from_str_radix(&profile[2..4], 16)? & 0x40 != 0);
            anyhow::ensure!(rtp_caps.get::<String>("packetization-mode")? == "1");
            println!(
                "PASS: synthetic NVENC 1920x1080@60 constrained-baseline; RTP profile-level-id={profile}, packetization-mode=1"
            );
            Ok(())
        })();
        // Stop GPU work even if a caps assertion or encoder operation failed.
        let stopped = pipeline.set_state(gst::State::Null);
        result.unwrap();
        stopped.unwrap();
    }

    #[test]
    fn gstreamer_accepts_browser_turn_urls_with_escaped_credentials() {
        gst::init().unwrap();
        let peer = gst::ElementFactory::make("webrtcbin").build().unwrap();
        let ice = crate::connectivity::IceConfiguration {
            ice_servers: vec![crate::connectivity::IceServer {
                urls: vec![
                    "turn:relay.example.com:3478?transport=udp".into(),
                    "turn:relay.example.com:3478?transport=tcp".into(),
                    "turns:relay.example.com:443?transport=tcp".into(),
                ],
                username: Some("session:123".into()),
                credential: Some("secret/+=".into()),
            }],
        };
        for uri in crate::connectivity::gstreamer_turn_uris(&ice).unwrap() {
            assert!(peer.emit_by_name::<bool>("add-turn-server", &[&uri]));
        }
        // The element never enters Playing: no relay requests or capture occur.
    }
}
