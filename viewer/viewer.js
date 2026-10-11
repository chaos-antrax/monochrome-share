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
let pc, socket, retryTimer, statsTimer, wakeLock, disconnectTimer;
let generation = 0, attempts = 0, started = false, stopped = false;
let lastSample;

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
    fail('Unable to reach the host. Check its address and paste the current access token to try again.');
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
    connect().catch(() => fail('Unable to initialize the viewer. Reload this page and try again.'));
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
  if (!window.RTCPeerConnection) { fail('This browser does not support WebRTC. Use Android Chrome or iPad Safari.'); return; }
  stopped = false;
  watch.disabled = true;
  setStatus('Connecting', 'Establishing your private stream…');
  const peer = pc = new RTCPeerConnection({ iceServers: [] });
  const stream = new MediaStream();
  video.srcObject = stream;
  // Called from the user's tap on the initial connection.
  video.play().catch(() => {});
  const url = new URL('/ws', location.href);
  url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  const ws = socket = new WebSocket(url);
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
    video.play().catch(() => {
      if (current !== generation) return;
      overlay.hidden = false;
      watch.disabled = false;
      watch.textContent = 'Tap to play ↗';
      setStatus('Tap to play', 'Your browser needs another tap to start playback with sound.');
    });
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
    signalQueue = signalQueue.then(async () => {
      if (current !== generation) return;
      const signal = JSON.parse(data);
      if (signal.type === 'error') { fail(signal.message); return; }
      if (signal.type === 'ice-config') {
        peer.setConfiguration({ iceServers: signal.ice_servers });
      } else if (signal.type === 'offer') {
        await peer.setRemoteDescription({ type: 'offer', sdp: signal.sdp });
        if (current !== generation) return;
        for (const candidate of pendingIce) await peer.addIceCandidate(candidate);
        pendingIce = [];
        const answer = await peer.createAnswer();
        await peer.setLocalDescription(answer);
        if (current !== generation) return;
        send({ type: 'answer', sdp: answer.sdp });
      } else if (signal.type === 'ice') {
        const candidate = { candidate: signal.candidate, sdpMLineIndex: signal.sdp_mline_index };
        if (peer.remoteDescription) await peer.addIceCandidate(candidate);
        else pendingIce.push(candidate);
      }
    }).catch(() => { if (current === generation) retry('Stream negotiation failed. Retrying…'); });
  };
  ws.onclose = () => { if (current === generation) retry('Host unavailable, token rejected, or another viewer is connected. Retrying…'); };
  ws.onerror = () => { /* onclose performs the reconnect. */ };
  statsTimer = setInterval(async () => {
    try {
      const reports = await peer.getStats();
      if (current !== generation) return;
      let videoReport, candidateReport;
      reports.forEach(report => {
        if (report.type === 'inbound-rtp' && report.kind === 'video') videoReport = report;
        if (report.type === 'candidate-pair' && report.state === 'succeeded' && report.nominated) candidateReport = report;
      });
      if (!videoReport) return;
      const report = videoReport;
      const elapsed = lastSample ? report.timestamp - lastSample.timestamp : 0;
      const mbps = elapsed > 0 ? ((report.bytesReceived - lastSample.bytesReceived) * 8 / elapsed / 1000).toFixed(1) : '—';
      lastSample = report;
      const fps = report.framesPerSecond ?? '—';
      const rtt = candidateReport?.currentRoundTripTime;
      const localCandidate = candidateReport ? reports.get(candidateReport.localCandidateId) : undefined;
      const remoteCandidate = candidateReport ? reports.get(candidateReport.remoteCandidateId) : undefined;
      const route = localCandidate?.candidateType === 'relay' || remoteCandidate?.candidateType === 'relay' ? 'Relay' : 'Direct';
      stats.textContent = `${report.frameWidth ?? '—'} × ${report.frameHeight ?? '—'} · ${fps} fps · ${mbps} Mbps · RTT ${rtt === undefined ? '—' : Math.round(rtt * 1000)} ms · ${candidateReport ? route : 'Connecting'}`;
    } catch { /* Session closed while collecting stats. */ }
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
    video.play().catch(() => setStatus('Playback blocked', 'Tap again to allow playback.'));
    send({ type: 'keyframe' });
    return;
  }
  started = true;
  attempts = 0;
  connect().catch(() => fail('Unable to initialize the viewer. Reload this page and try again.'));
});
video.addEventListener('playing', () => {
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
    video.play().catch(() => { overlay.hidden = false; watch.disabled = false; watch.textContent = 'Tap to play ↗'; });
  } else retry();
});
window.addEventListener('online', () => { if (started && !stopped && pc?.connectionState !== 'connected') retry(); });
window.addEventListener('pagehide', () => { generation += 1; clearTimeout(retryTimer); retryTimer = null; dispose(); });
window.addEventListener('pageshow', event => { if (event.persisted && started && !stopped) retry(); });
audioLabel();
