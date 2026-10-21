import { acceptsVideo, summarizeStats, startupDiagnosis, browserErrorName } from './connection.js';

const video = document.querySelector('#screen');
const status = document.querySelector('#status');
const message = document.querySelector('#message');
const overlay = document.querySelector('#overlay');
const watch = document.querySelector('#watch');
const mute = document.querySelector('#mute');
const stats = document.querySelector('#stats');
const access = document.querySelector('#access');
const tokenInput = document.querySelector('#token');
const stage = document.querySelector('#stage');
const controls = document.querySelector('#controls');
const details = document.querySelector('#stream-details');
const fullscreen = document.querySelector('#fullscreen');
const volume = document.querySelector('#volume');
let controlsTimer;

function showControls() {
  clearTimeout(controlsTimer);
  controls.classList.remove('is-hidden');
  if (!overlay.hidden || video.paused) return;
  controlsTimer = setTimeout(() => {
    if (!overlay.hidden || video.paused || details.open || controls.contains(document.activeElement)) return;
    controls.classList.add('is-hidden');
  }, 3500);
}
function fullscreenLabel() {
  const active = Boolean(document.fullscreenElement || video.webkitDisplayingFullscreen);
  stage.classList.toggle('is-fullscreen', active);
  const label = active ? 'Exit fullscreen' : 'Enter fullscreen';
  fullscreen.setAttribute('aria-label', label);
  fullscreen.title = label;
  showControls();
}

let token = new URLSearchParams(location.hash.slice(1)).get('token') || '';
// Keep the secret in memory, out of referrers and the visible address bar.
if (location.hash) history.replaceState(null, '', location.pathname + location.search);
access.hidden = Boolean(token);
let pc, socket, retryTimer, statsTimer, wakeLock, disconnectTimer, startupTimer;
let generation = 0, attempts = 0, started = false, stopped = false;
let lastSample, startupState;
const STARTUP_TIMEOUT_MS = 45000;

function startupSucceeded() {
  // Audio alone can fire playing; only video can cancel the video-start deadline.
  if (video.videoWidth > 0 && !video.paused) {
    clearTimeout(startupTimer);
    if (startupState) startupState.videoStarted = true;
    return true;
  }
  return false;
}
function initializationFailed(error) {
  fail(`Unable to initialize the viewer (${browserErrorName(error)}). Reload the page and try again.`);
}
function playbackBlocked() {
  if (startupState) startupState.playbackBlocked = true;
  overlay.hidden = false;
  watch.disabled = false;
  watch.textContent = 'Tap to play ↗';
  setStatus('Tap to play', 'Your browser needs another tap to start playback with sound.');
}
function audioLabel() {
  const silent = video.muted || video.volume === 0;
  const label = silent ? 'Unmute audio' : 'Mute audio';
  mute.setAttribute('aria-pressed', String(silent));
  mute.setAttribute('aria-label', label);
  mute.title = label;
}
function setStatus(text, detail) {
  status.textContent = text;
  status.parentElement.title = text;
  if (detail) message.textContent = detail;
  showControls();
}
function send(signal) {
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(signal));
}
function dispose() {
  clearTimeout(controlsTimer);
  controls.classList.remove('is-hidden');
  stage.classList.remove('is-live');
  clearInterval(statsTimer);
  clearTimeout(disconnectTimer);
  clearTimeout(startupTimer);
  if (socket) { socket.onclose = null; socket.close(); socket = null; }
  if (pc) { pc.onconnectionstatechange = null; pc.close(); pc = null; }
  video.srcObject = null;
  lastSample = undefined;
  if (wakeLock) { wakeLock.release().catch(() => {}); wakeLock = null; }
}
async function keepAwake() {
  if (document.visibilityState !== 'visible' || !navigator.wakeLock || wakeLock) return;
  try {
    const lock = await navigator.wakeLock.request('screen');
    if (stopped || !pc) { await lock.release(); return; }
    wakeLock = lock;
    lock.addEventListener('release', () => { if (wakeLock === lock) wakeLock = null; });
  } catch { /* Unsupported, denied, or low battery: playback remains available. */ }
}
function retry(detail = 'Connection interrupted. Reconnecting automatically…') {
  if (stopped || retryTimer) return;
  if (attempts >= 5) {
    token = '';
    access.hidden = false;
    fail(`Unable to connect after several attempts. ${detail} Check the host and paste the current access token to try again.`);
    return;
  }
  generation += 1;
  dispose();
  overlay.hidden = false;
  watch.disabled = false;
  watch.textContent = 'Reconnect now ↗';
  setStatus('Reconnecting', detail);
  const delay = Math.min(15000, 1000 * 2 ** Math.min(attempts++, 4)) + Math.random() * 300;
  retryTimer = setTimeout(() => {
    retryTimer = null;
    connect().catch(initializationFailed);
  }, delay);
}
function fail(detail) {
  stopped = true;
  generation += 1;
  clearTimeout(retryTimer); retryTimer = null;
  dispose();
  overlay.hidden = false;
  watch.disabled = false;
  watch.textContent = 'Try again ↗';
  setStatus('Unable to stream', detail);
  stats.textContent = 'No active stream';
}

async function connect() {
  clearTimeout(retryTimer); retryTimer = null;
  generation += 1;
  const current = generation;
  dispose();
  if (!window.RTCPeerConnection) { fail('This browser does not support WebRTC. Use an updated browser.'); return; }
  stopped = false;
  watch.disabled = true;
  setStatus('Connecting', 'Establishing your private stream…');
  // Only the current connection's health is retained. No SDP, timeline, or reports.
  const state = startupState = { step: 'creating the connection', websocket: 'connecting',
    answerSent: false, videoStarted: false, playbackBlocked: false, media: summarizeStats(new Map()) };
  const peer = pc = new RTCPeerConnection({ iceServers: [] });
  startupTimer = setTimeout(() => {
    if (current !== generation || state.videoStarted || startupSucceeded()) return;
    state.connection = peer.connectionState;
    state.ice = peer.iceConnectionState;
    const diagnosis = startupDiagnosis(state);
    if (state.playbackBlocked && state.media.video.bytes > 0 &&
        state.media.video.framesDecoded !== 0 && peer.connectionState === 'connected') {
      // Preserve a valid media connection for another user gesture.
      playbackBlocked();
    } else fail(`Startup timed out after 45 seconds. ${diagnosis}`);
  }, STARTUP_TIMEOUT_MS);
  const stream = new MediaStream();
  video.srcObject = stream;
  // Prime playback within the user's initial tap; tracks arrive asynchronously.
  video.play().catch(() => { if (current === generation) state.playbackBlocked = true; });
  const url = new URL('/ws', location.href);
  url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  const ws = socket = new WebSocket(url);
  ws.onopen = () => {
    if (current !== generation) return;
    state.websocket = 'open';
    state.step = 'waiting for the host offer';
  };
  let pendingIce = [], signalQueue = Promise.resolve();
  peer.onicecandidate = ({ candidate }) => {
    if (current !== generation || !candidate) return;
    send({ type: 'ice', candidate: candidate.candidate, sdp_mline_index: candidate.sdpMLineIndex ?? 0 });
  };
  peer.ontrack = ({ track, receiver }) => {
    if (current !== generation) return;
    stream.addTrack(track);
    if ('jitterBufferTarget' in receiver) {
      try { receiver.jitterBufferTarget = 30; } catch { /* Browser-specific hint. */ }
    }
    video.play().catch(() => { if (current === generation) playbackBlocked(); });
  };
  peer.onconnectionstatechange = () => {
    if (current !== generation) return;
    if (peer.connectionState === 'connected') {
      clearTimeout(disconnectTimer);
      setStatus('Connected');
      keepAwake();
    } else if (peer.connectionState === 'failed') retry();
    else if (peer.connectionState === 'disconnected') {
      clearTimeout(disconnectTimer);
      disconnectTimer = setTimeout(() => { if (current === generation) retry(); }, 3500);
    }
  };
  ws.onmessage = ({ data }) => {
    // SDP must finish before adding ICE. Serialize asynchronous message handling.
    let operation = 'reading host signaling';
    signalQueue = signalQueue.then(async () => {
      if (current !== generation) return;
      const signal = JSON.parse(data);
      if (signal.type === 'error') { fail(signal.message); return; }
      if (signal.type === 'ice-config') {
        state.step = operation = 'applying connection settings';
        peer.setConfiguration({ iceServers: signal.ice_servers });
      } else if (signal.type === 'offer') {
        state.step = operation = 'applying the host offer';
        await peer.setRemoteDescription({ type: 'offer', sdp: signal.sdp });
        if (current !== generation) return;
        state.step = operation = 'applying queued connection candidates';
        for (const candidate of pendingIce) {
          await peer.addIceCandidate(candidate);
          if (current !== generation) return;
        }
        pendingIce = [];
        state.step = operation = 'creating the browser answer';
        const answer = await peer.createAnswer();
        if (current !== generation) return;
        if (!acceptsVideo(answer.sdp)) {
          fail('This browser did not accept the video format. Update the browser and host app, then restart sharing.');
          return;
        }
        state.step = operation = 'applying the browser answer';
        await peer.setLocalDescription(answer);
        if (current !== generation) return;
        send({ type: 'answer', sdp: answer.sdp });
        state.answerSent = true;
        state.step = 'waiting for media';
      } else if (signal.type === 'ice') {
        const candidate = { candidate: signal.candidate, sdpMLineIndex: signal.sdp_mline_index };
        if (peer.remoteDescription) {
          operation = 'applying a connection candidate';
          await peer.addIceCandidate(candidate);
        } else pendingIce.push(candidate);
      }
    }).catch(error => {
      if (current !== generation) return;
      retry(`Stream negotiation failed while ${operation} (${browserErrorName(error)}). Reconnecting…`);
    });
  };
  ws.onclose = () => {
    if (current !== generation) return;
    state.websocket = 'closed';
    retry('Host unavailable, token rejected, or another viewer is connected. Retrying…');
  };
  ws.onerror = () => { /* onclose performs the reconnect. */ };
  let statsPending = false;
  statsTimer = setInterval(async () => {
    if (statsPending) return;
    statsPending = true;
    try {
      const reports = await peer.getStats();
      if (current !== generation) return;
      state.media = summarizeStats(reports);
      startupSucceeded();
      let videoReport;
      reports.forEach(report => {
        if (report.type === 'inbound-rtp' && !report.isRemote && (report.kind || report.mediaType) === 'video') videoReport = report;
      });
      if (!videoReport) { stats.textContent = 'Waiting for video'; return; }
      const report = videoReport;
      const elapsed = lastSample ? report.timestamp - lastSample.timestamp : 0;
      const mbps = elapsed > 0 ? ((report.bytesReceived - lastSample.bytesReceived) * 8 / elapsed / 1000).toFixed(1) : '—';
      lastSample = report;
      const fps = report.framesPerSecond ?? '—';
      const rtt = state.media.rttMs ?? '—';
      stats.textContent = `${report.frameWidth ?? '—'} × ${report.frameHeight ?? '—'} · ${fps} fps · ${mbps} Mbps · RTT ${rtt} ms · ${state.media.route}`;
    } catch { /* Stats are optional; a closed/unsupported receiver must not break playback. */ }
    finally { statsPending = false; }
  }, 2000);
}

watch.addEventListener('click', () => {
  if (!token) {
    if (!access.reportValidity()) return;
    token = tokenInput.value.trim();
    tokenInput.value = '';
    access.hidden = true;
  }
  video.muted = false;
  audioLabel();
  if (pc?.connectionState === 'connected') {
    const current = generation;
    video.play().catch(() => { if (current === generation) playbackBlocked(); });
    send({ type: 'keyframe' });
    return;
  }
  started = true;
  attempts = 0;
  connect().catch(initializationFailed);
});
video.addEventListener('playing', () => {
  if (startupState) startupState.playbackBlocked = false;
  startupSucceeded();
  overlay.hidden = true;
  stage.classList.add('is-live');
  watch.disabled = false;
  attempts = 0;
  setStatus('Live');
  keepAwake();
});
mute.addEventListener('click', () => {
  if (video.volume === 0) {
    video.volume = 1;
    volume.value = '1';
    video.muted = false;
  } else video.muted = !video.muted;
  video.play().catch(() => {});
  audioLabel();
});
volume.addEventListener('input', event => {
  video.volume = Number(event.target.value);
  video.muted = video.volume === 0;
  audioLabel();
  showControls();
});
fullscreen.addEventListener('click', async () => {
  try {
    if (document.fullscreenElement) await document.exitFullscreen();
    else if (stage.requestFullscreen) await stage.requestFullscreen();
    else if (video.webkitEnterFullscreen) video.webkitEnterFullscreen();
  } catch { setStatus('Fullscreen unavailable'); }
});
document.addEventListener('fullscreenchange', fullscreenLabel);
video.addEventListener('webkitbeginfullscreen', fullscreenLabel);
video.addEventListener('webkitendfullscreen', fullscreenLabel);
video.addEventListener('pause', showControls);
video.addEventListener('error', () => {
  if (started && !stopped) fail('The browser could not play this stream. Update the browser and restart sharing.');
});
stage.addEventListener('pointermove', showControls);
stage.addEventListener('pointerdown', showControls);
controls.addEventListener('focusin', showControls);
controls.addEventListener('focusout', () => setTimeout(showControls, 0));
details.addEventListener('toggle', showControls);
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && details.open) { details.open = false; showControls(); }
});
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState !== 'visible' || !started || stopped) return;
  if (pc?.connectionState === 'connected') {
    send({ type: 'keyframe' });
    keepAwake();
    const current = generation;
    video.play().catch(() => { if (current === generation) playbackBlocked(); });
  } else retry();
});
window.addEventListener('online', () => { if (started && !stopped && pc?.connectionState !== 'connected') retry(); });
window.addEventListener('pagehide', () => { generation += 1; clearTimeout(retryTimer); retryTimer = null; dispose(); });
window.addEventListener('pageshow', event => { if (event.persisted && started && !stopped) retry(); });
audioLabel();
