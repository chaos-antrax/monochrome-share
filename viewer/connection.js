// Small connection guards shared by the viewer and regression tests.
// No diagnostic history, browser fingerprint, raw error messages, or report export.
export function summarizeSdp(sdp) {
  const sections = String(sdp).split(/(?=^m=)/m);
  const sessionLines = (sections[0]?.startsWith('m=') ? '' : sections[0] || '').split(/\r?\n/);
  const groups = sessionLines.filter(line => line.startsWith('a=group:BUNDLE '))
    .map(line => line.slice('a=group:BUNDLE '.length).trim().split(/\s+/));
  return sections.filter(section => section.startsWith('m=')).map(section => {
    const lines = section.split(/\r?\n/);
    const [, kind, port] = /^m=(audio|video)\s+(\d+)/.exec(lines[0]) || [];
    if (!kind) return null;
    const mid = lines.find(line => line.startsWith('a=mid:'))?.slice(6).trim();
    const bundled = groups.some(group => mid && group.includes(mid));
    const bundleOnly = lines.includes('a=bundle-only');
    const codecs = lines.flatMap(line => {
      const match = /^a=rtpmap:\d+ ([\w.+-]+)\//.exec(line);
      return match ? [`${kind}/${match[1]}`] : [];
    });
    // RFC 8843: a zero-port bundle-only section in a BUNDLE group is not rejected.
    return { kind, rejected: port === '0' && !(bundleOnly && bundled),
      direction: [...lines, ...sessionLines].find(line => /^a=(sendrecv|sendonly|recvonly|inactive)$/.test(line))?.slice(2) || 'sendrecv', codecs };
  }).filter(Boolean);
}

export function acceptsVideo(sdp) {
  return summarizeSdp(sdp).some(section => section.kind === 'video' && !section.rejected &&
    ['recvonly', 'sendrecv'].includes(section.direction) && section.codecs.length);
}

export function summarizeStats(reports) {
  const all = [...reports.values()];
  const transport = all.find(report => report.type === 'transport' && report.selectedCandidatePairId);
  const pair = (transport && reports.get(transport.selectedCandidatePairId)) || all.find(report =>
    report.type === 'candidate-pair' && report.state === 'succeeded' && (report.nominated || report.selected));
  const local = pair && reports.get(pair.localCandidateId);
  const remote = pair && reports.get(pair.remoteCandidateId);
  const media = kind => {
    const entries = all.filter(report => report.type === 'inbound-rtp' && !report.isRemote && (report.kind || report.mediaType) === kind);
    const sum = field => entries.some(report => Number.isFinite(report[field]))
      ? entries.reduce((total, report) => total + (Number.isFinite(report[field]) ? report[field] : 0), 0) : null;
    return { bytes: sum('bytesReceived') ?? 0, framesDecoded: sum('framesDecoded') };
  };
  return { audio: media('audio'), video: media('video'),
    dtls: all.find(report => report.type === 'transport' && report.dtlsState)?.dtlsState || 'unreported',
    route: pair ? (local?.candidateType === 'relay' || remote?.candidateType === 'relay' ? 'Relay' : 'Direct') : 'Connecting',
    rttMs: Number.isFinite(pair?.currentRoundTripTime) ? Math.round(pair.currentRoundTripTime * 1000) : null };
}

export function startupDiagnosis(state) {
  if (state.websocket !== 'open') return 'The host could not be reached. Check its address and use a fresh viewer link.';
  if (!state.answerSent) return `Stream setup did not finish while ${state.step}. Restart sharing and try again.`;
  if (state.connection !== 'connected' && !['connected', 'completed'].includes(state.ice)) return 'A media connection could not be established. Check the network and the host’s remote-sharing settings.';
  if (state.media.dtls !== 'connected' && state.media.dtls !== 'unreported') return 'The secure media connection did not finish. Reconnect or restart sharing.';
  if (!state.media.video.bytes && !state.media.audio.bytes) return 'Connected, but no media was received. Check that the host is still sharing and reconnect.';
  if (!state.media.video.bytes) return 'Audio arrived, but video did not. Restart sharing and try again.';
  if (state.media.video.framesDecoded === 0) return 'Video arrived but could not be decoded. Try an updated browser and restart sharing.';
  if (state.playbackBlocked) return 'Your browser blocked playback. Tap again to start watching.';
  return 'Video did not start. Reconnect or restart sharing.';
}

export function browserErrorName(error) {
  // Browser exception messages can contain signaling URLs or credential-bearing
  // ICE configuration. Only display recognized exception names, never raw messages.
  return ['OperationError', 'NotSupportedError', 'InvalidStateError', 'InvalidAccessError',
    'NotAllowedError', 'SecurityError', 'TypeError', 'SyntaxError', 'AbortError', 'NetworkError']
    .includes(error?.name) ? error.name : 'Browser error';
}
