// Deterministic connection regressions; no GPU, external network, or device claims.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { acceptsVideo, summarizeSdp, summarizeStats, startupDiagnosis, browserErrorName } from '../viewer/connection.js';

const offer = 'v=0\r\na=group:BUNDLE video0 audio1\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:video0\r\na=sendonly\r\na=rtpmap:96 H264/90000\r\na=fmtp:96 profile-level-id=42c02a;packetization-mode=1\r\nm=audio 0 UDP/TLS/RTP/SAVPF 97\r\na=mid:audio1\r\na=bundle-only\r\na=rtpmap:97 OPUS/48000/2\r\n';
const answer = offer.replace('sendonly', 'recvonly');
const rejectedVideo = 'v=0\r\na=group:BUNDLE audio1\r\nm=video 0 UDP/TLS/RTP/SAVPF 96\r\na=mid:video0\r\na=recvonly\r\nm=audio 9 UDP/TLS/RTP/SAVPF 97\r\na=mid:audio1\r\na=recvonly\r\na=rtpmap:97 OPUS/48000/2\r\n';
const bundledVideo = answer.replace('a=group:BUNDLE video0 audio1', 'a=group:BUNDLE audio1 video0')
  .replace('m=video 9', 'm=video 0').replace('a=mid:video0\r\n', 'a=mid:video0\r\na=bundle-only\r\n')
  .replace('m=audio 0', 'm=audio 9').replace('a=mid:audio1\r\na=bundle-only', 'a=mid:audio1');
assert.equal(acceptsVideo(answer), true);
assert.equal(summarizeSdp(offer)[1].rejected, false, 'BUNDLE-only audio is not rejected');
assert.equal(summarizeSdp(offer.replace('a=bundle-only\r\n', ''))[1].rejected, true);
assert.equal(summarizeSdp(offer.replace('a=group:BUNDLE video0 audio1\r\n', ''))[1].rejected, true);
assert.equal(acceptsVideo(bundledVideo), true, 'BUNDLE-only video remains valid');
assert.equal(acceptsVideo(rejectedVideo), false);
assert.equal(acceptsVideo('m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=inactive\r\na=rtpmap:96 H264/90000\r\n'), false);
assert.equal(acceptsVideo('m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=sendonly\r\na=rtpmap:96 H264/90000\r\n'), false);
assert.equal(acceptsVideo('m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=recvonly\r\n'), false);
assert.equal(acceptsVideo(answer.replace('a=recvonly\r\n', '').replace('v=0\r\n', 'v=0\r\na=recvonly\r\n')), true, 'Session direction is inherited');

const reports = new Map([
  ['video', { type: 'inbound-rtp', mediaType: 'video', bytesReceived: 1200, framesDecoded: 0, frameWidth: 1920, frameHeight: 1080, framesPerSecond: 60, timestamp: 1000 }],
  ['audio', { type: 'inbound-rtp', kind: 'audio', bytesReceived: 300 }],
  ['transport', { type: 'transport', dtlsState: 'connected', selectedCandidatePairId: 'pair' }],
  ['pair', { type: 'candidate-pair', state: 'succeeded', localCandidateId: 'local', remoteCandidateId: 'remote', currentRoundTripTime: .015 }],
  ['local', { candidateType: 'relay', address: '1.2.3.4', url: 'turn:private' }],
  ['remote', { candidateType: 'host', address: '5.6.7.8' }],
]);
const media = summarizeStats(reports);
assert.equal(media.audio.bytes, 300);
assert.equal(media.video.bytes, 1200);
assert.equal(media.video.framesDecoded, 0);
assert.equal(media.audio.framesDecoded, null, 'Missing counters must not become zero');
assert.equal(media.route, 'Relay');
assert.equal(media.rttMs, 15);
assert.ok(!JSON.stringify(media).match(/1\.2\.3\.4|5\.6\.7\.8|turn:private/));
const state = { websocket: 'open', answerSent: true, connection: 'connected', ice: 'connected', media };
assert.match(startupDiagnosis(state), /could not be decoded/);
assert.match(startupDiagnosis({ ...state, media: summarizeStats(new Map()) }), /no media/);
assert.match(startupDiagnosis({ ...state, connection: 'connecting', ice: 'checking' }), /media connection/);
assert.match(startupDiagnosis({ ...state, answerSent: false, step: 'applying the host offer' }), /host offer/);
assert.match(startupDiagnosis({ ...state, websocket: 'connecting' }), /host could not be reached/);
assert.match(startupDiagnosis({ ...state, media: { ...media, dtls: 'connecting' } }), /secure media/);
assert.equal(browserErrorName({ name: 'OperationError', message: 'https://secret/?token=private' }), 'OperationError');
assert.equal(browserErrorName({ name: 'secret', message: 'private' }), 'Browser error');

function fixture() {
  const elements = new Map(), timers = new Map(), peers = [], sockets = [];
  let timerId = 0;
  const element = id => {
    if (!elements.has(id)) elements.set(id, {
      hidden: false, value: '', textContent: '', disabled: false, paused: true,
      readyState: 0, networkState: 0, videoWidth: 0, videoHeight: 0, currentTime: 0,
      volume: 1, muted: true, handlers: {}, classList: { add() {}, remove() {}, toggle() {} },
      parentElement: {}, contains: () => false, setAttribute() {}, reportValidity: () => true,
      addEventListener(name, fn) { (this.handlers[name] ||= []).push(fn); },
      async fire(name, event = {}) { for (const fn of this.handlers[name] || []) await fn({ currentTarget: this, ...event }); },
      play() { return Promise.resolve(); },
    });
    return elements.get(id);
  };
  const document = { querySelector: element, visibilityState: 'visible', activeElement: null, addEventListener() {} };
  class Peer {
    constructor() {
      Object.assign(this, { connectionState: 'new', iceConnectionState: 'new' });
      peers.push(this);
    }
    close() { this.closed = true; }
    setConfiguration() {}
    async setRemoteDescription(description) { this.remoteDescription = description; }
    async createAnswer() { return { type: 'answer', sdp: answer }; }
    async setLocalDescription() {}
    async addIceCandidate() {}
    async getStats() { return this.reports || new Map(); }
  }
  class Socket {
    static OPEN = 1;
    constructor() { this.readyState = 1; sockets.push(this); }
    close() {}
    send(value) { (this.sent ||= []).push(JSON.parse(value)); }
  }
  const context = { document, window: { RTCPeerConnection: Peer, addEventListener() {} }, navigator: {},
    location: { hash: '#token=synthetic-access-secret', pathname: '/', search: '', protocol: 'http:', href: 'http://127.0.0.1/' },
    history: { replaceState() {} }, URL, URLSearchParams, Map, Set,
    RTCPeerConnection: Peer, WebSocket: Socket, MediaStream: class { addTrack() {} },
    setTimeout(fn, delay) { const id = ++timerId; timers.set(id, { fn, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
    setInterval(fn, delay) { const id = ++timerId; timers.set(id, { fn, delay, interval: true }); return id; },
    clearInterval(id) { timers.delete(id); },
    acceptsVideo, summarizeStats, startupDiagnosis, browserErrorName,
  };
  const source = readFileSync(new URL('../viewer/viewer.js', import.meta.url), 'utf8').replace(/^import .*;\r?\n/, '');
  assert.ok(!/diagnostic|userAgent|clipboard|console\./i.test(source), 'Temporary report collection and export must be removed');
  runInNewContext(source, context);
  const tick = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
  const start = async () => { await element('#watch').fire('click'); sockets.at(-1).onopen(); await tick(); };
  const signal = async value => { sockets.at(-1).onmessage({ data: JSON.stringify(value) }); await tick(); };
  const timeout = async () => {
    const timer = [...timers.entries()].find(([, timer]) => timer.delay === 45000);
    assert.ok(timer, 'Startup deadline must exist');
    timers.delete(timer[0]); timer[1].fn(); await tick();
  };
  const sample = async () => { await [...timers.values()].find(timer => timer.interval).fn(); await tick(); };
  return { element, peers, sockets, timers, start, signal, timeout, sample, tick };
}

{
  const f = fixture();
  await f.start(); await f.signal({ type: 'offer', sdp: offer });
  assert.equal(f.sockets[0].sent[0].type, 'answer');
  await f.timeout();
  assert.equal(f.peers[0].closed, true);
  assert.equal(f.element('#watch').disabled, false);
  assert.match(f.element('#message').textContent, /media connection/);
}
{
  const f = fixture();
  await f.start();
  f.peers[0].createAnswer = async () => ({ type: 'answer', sdp: rejectedVideo });
  await f.signal({ type: 'offer', sdp: offer });
  assert.equal(f.peers[0].closed, true);
  assert.equal(f.sockets[0].sent?.length || 0, 0);
  assert.match(f.element('#message').textContent, /did not accept the video format/);
  assert.ok(![...f.timers.values()].some(timer => timer.delay === 45000));
}
{
  const f = fixture();
  await f.start();
  f.peers[0].createAnswer = async () => ({ type: 'answer', sdp: bundledVideo });
  await f.signal({ type: 'offer', sdp: offer });
  assert.equal(f.sockets[0].sent[0].type, 'answer');
  assert.ok(!f.peers[0].closed);
}
{
  const f = fixture();
  await f.start();
  f.peers[0].setRemoteDescription = async () => { const error = new Error('https://private/ws?token=synthetic-access-secret'); error.name = 'OperationError'; throw error; };
  await f.signal({ type: 'offer', sdp: offer });
  assert.match(f.element('#message').textContent, /applying the host offer \(OperationError\)/);
  assert.ok(!f.element('#message').textContent.match(/synthetic-access-secret|https:\/\/private/));
  const retry = [...f.timers.values()].find(timer => timer.delay >= 1000 && timer.delay < 1400);
  assert.ok(retry);
  retry.fn(); await f.tick();
  assert.equal(f.peers.length, 2, 'Bounded reconnection is retained');
}
{
  const f = fixture();
  await f.start(); await f.signal({ type: 'offer', sdp: offer });
  f.peers[0].connectionState = 'connected'; f.peers[0].iceConnectionState = 'connected'; f.peers[0].reports = reports;
  await f.sample();
  assert.match(f.element('#stats').textContent, /1920 × 1080 · 60 fps/);
  assert.match(f.element('#stats').textContent, /RTT 15 ms · Relay/);
  await f.element('#screen').fire('playing'); // Audio-only playing must not disable the video timeout.
  await f.timeout();
  assert.match(f.element('#message').textContent, /could not be decoded/);
}
{
  const f = fixture();
  await f.start();
  f.element('#screen').videoWidth = 1920; f.element('#screen').videoHeight = 1080; f.element('#screen').paused = false;
  await f.element('#screen').fire('playing');
  assert.ok(![...f.timers.values()].some(timer => timer.delay === 45000));
  assert.equal(f.element('#status').textContent, 'Live');
}
{
  const f = fixture();
  await f.start(); await f.signal({ type: 'offer', sdp: offer });
  const video = f.element('#screen');
  video.play = () => Promise.reject(new Error('Synthetic playback block'));
  f.peers[0].connectionState = 'connected';
  f.peers[0].reports = new Map(reports);
  f.peers[0].reports.set('video', { ...reports.get('video'), framesDecoded: 5 });
  f.peers[0].ontrack({ track: { kind: 'video' }, receiver: {} }); await f.tick(); await f.sample();
  await f.timeout();
  assert.ok(!f.peers[0].closed, 'Keep a usable peer for a second playback gesture');
  assert.equal(f.element('#watch').disabled, false);
  assert.match(f.element('#message').textContent, /another tap/);
}
{
  const f = fixture();
  await f.start();
  let finishAnswer;
  f.peers[0].createAnswer = () => new Promise(resolve => { finishAnswer = resolve; });
  await f.signal({ type: 'offer', sdp: offer });
  await f.timeout();
  finishAnswer({ type: 'answer', sdp: answer }); await f.tick();
  assert.equal(f.sockets[0].sent?.length || 0, 0, 'Late SDP must not revive a timed-out peer');
}
console.log('PASS: BUNDLE audio/video, codec rejection, Safari statistics, startup timeouts/cleanup, safe negotiation errors, retries, normal stream statistics, video-start detection, playback recovery and stale SDP guards');
