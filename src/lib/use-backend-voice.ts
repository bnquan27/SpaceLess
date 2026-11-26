import { useEffect, useRef, useState } from 'react';
import type { Room, Participant } from 'livekit-client';
import { ApiError, type ApiClient, type ApiEvent, type Bootstrap } from './api';
import type { ScreenShareSession } from '@/components/ui/workspace/types';

type ActiveVoice = { roomId: string; sessionId: string; live: Room };
export function useBackendVoice(api: ApiClient, data: Bootstrap | null, subscribe: (listener: (event: ApiEvent) => void) => () => void, report: (error: unknown) => void) {
  const active = useRef<ActiveVoice | null>(null);
  const generation = useRef(0);
  const latest = useRef({ data, report }); latest.current = { data, report };
  const [roomId, setRoomId] = useState('');
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [micMuted, setMicMuted] = useState(true);
  const [cameraOn, setCameraOn] = useState(false);
  const [shares, setShares] = useState<ScreenShareSession[]>([]);
  const [participants, setParticipants] = useState<{init:string;name:string;badge:string;speaking:boolean;sub:string}[]>([]);
  const audio = useRef(new Set<HTMLMediaElement>());
  const audioCleanup = () => { audio.current.forEach(element => element.remove()); audio.current.clear(); };
  const clear = () => { setRoomId(''); setConnected(false); setBusy(false); setShares([]); setParticipants([]); setMicMuted(true); setCameraOn(false); audioCleanup(); };
  const revoke = async (voice: ActiveVoice) => {
    await voice.live.disconnect();
    await api.request(`/rooms/${voice.roomId}/voice/leave?voice_session_id=${voice.sessionId}`, 'DELETE');
  };
  async function leave() {
    generation.current++;
    const previous = active.current; active.current = null; clear();
    if (previous) try { await revoke(previous); } catch (error) { latest.current.report(error); }
  }
  useEffect(() => {
    const unsubscribe = subscribe(event => {
      const voice = active.current;
      if (voice && event.type === 'VOICE_STATE_UPDATE' && event.data.voice_session_id === voice.sessionId && ['ENDED','REVOKING'].includes(event.data.state || '')) {
        active.current = null; generation.current++; clear(); void voice.live.disconnect();
      }
    });
    return () => { unsubscribe(); generation.current++; const voice = active.current; active.current = null; audioCleanup(); if (voice) void revoke(voice).catch(() => {}); };
  }, [api, subscribe]);
  async function join(target: string) {
    if (busy || (active.current?.roomId === target && connected)) return;
    await leave(); const version = ++generation.current; setBusy(true);
    let issued: ActiveVoice | null = null;
    try {
      const sdk = await import('livekit-client');
      if (version !== generation.current) return;
      const grant = await api.request<{voice_session_id:string;livekit_url:string;livekit_token:string}>(`/rooms/${target}/voice/join`, 'POST');
      const live = new sdk.Room({ adaptiveStream:true, dynacast:true });
      issued = {roomId:target,sessionId:grant.voice_session_id,live};
      if (version !== generation.current) { await revoke(issued); return; }
      active.current = issued;
      const nameOf = (person: Participant) => {
        const snapshot = latest.current.data;
        if (person === live.localParticipant) return snapshot?.profile.full_name || 'Bạn';
        const state = snapshot?.voice_states.find(state => person.identity === `voice-${state.voice_session_id}`);
        return snapshot?.friends.find(friend => friend.user_id === state?.user_id)?.full_name || snapshot?.channels.flatMap(channel => channel.members).find(member => member.user_id === state?.user_id)?.full_name || 'Người tham gia';
      };
      const update = () => {
        if (active.current?.sessionId !== grant.voice_session_id) return;
        const people = [live.localParticipant, ...live.remoteParticipants.values()];
        setParticipants(people.map(person => ({init:nameOf(person).split(/\s+/).slice(-2).map(part => Array.from(part)[0]).join(''),name:nameOf(person),speaking:person.isSpeaking,badge:person.isMicrophoneEnabled ? 'Mic đang bật' : 'Đã tắt mic',sub:person === live.localParticipant ? 'Bạn' : ''})));
        setMicMuted(!live.localParticipant.isMicrophoneEnabled); setCameraOn(live.localParticipant.isCameraEnabled);
        setShares(people.flatMap(person => { const publication = person.getTrackPublication(sdk.Track.Source.ScreenShare); const video = publication?.track?.mediaStreamTrack; if (!video) return []; return [{owner:nameOf(person),isLocal:person === live.localParticipant,stream:new MediaStream([video])}]; }));
      };
      live.on(sdk.RoomEvent.TrackSubscribed, track => { if (track.kind === sdk.Track.Kind.Audio && active.current?.sessionId === grant.voice_session_id) { const element = track.attach(); element.style.display='none'; document.body.appendChild(element); audio.current.add(element); } update(); });
      live.on(sdk.RoomEvent.TrackUnsubscribed, track => { track.detach().forEach(element => { audio.current.delete(element); element.remove(); }); update(); });
      [sdk.RoomEvent.LocalTrackPublished,sdk.RoomEvent.LocalTrackUnpublished,sdk.RoomEvent.ParticipantConnected,sdk.RoomEvent.ParticipantDisconnected,sdk.RoomEvent.ActiveSpeakersChanged,sdk.RoomEvent.TrackMuted,sdk.RoomEvent.TrackUnmuted].forEach(event => live.on(event,update));
      live.on(sdk.RoomEvent.Disconnected, () => { if (active.current?.sessionId === grant.voice_session_id) { active.current=null; clear(); latest.current.report(new Error('Phòng thoại đã ngắt kết nối. Vui lòng tham gia lại.')); } });
      live.on(sdk.RoomEvent.Reconnecting, () => { if (active.current?.sessionId === grant.voice_session_id) setConnected(false); });
      live.on(sdk.RoomEvent.Reconnected, () => { if (active.current?.sessionId === grant.voice_session_id) setConnected(true); });
      await live.connect(grant.livekit_url, grant.livekit_token);
      if (version !== generation.current) { await revoke(issued); return; }
      setRoomId(target); setConnected(true); update();
    } catch (error) {
      if (issued) { if (active.current?.sessionId === issued.sessionId) active.current=null; await revoke(issued).catch(() => {}); }
      if (version === generation.current) { clear(); latest.current.report(error instanceof ApiError && error.code === 'UNAVAILABLE' ? new Error('LiveKit chưa sẵn sàng. Hãy kiểm tra cấu hình máy chủ media trước khi gọi thoại hoặc chia sẻ màn hình.') : error); }
    } finally { if (version === generation.current) setBusy(false); }
  }
  async function mic() { const voice = active.current; if (!voice || !connected) return; try { await voice.live.localParticipant.setMicrophoneEnabled(!voice.live.localParticipant.isMicrophoneEnabled); setMicMuted(!voice.live.localParticipant.isMicrophoneEnabled); } catch (error) { latest.current.report(error); } }
  async function camera() { const voice = active.current; if (!voice || !connected) return; try { await voice.live.localParticipant.setCameraEnabled(!voice.live.localParticipant.isCameraEnabled); setCameraOn(voice.live.localParticipant.isCameraEnabled); } catch (error) { latest.current.report(error); } }
  async function listen(muted: boolean) { audio.current.forEach(element => { element.muted = muted; }); if (!muted) await active.current?.live.startAudio().catch(error => latest.current.report(error)); }
  async function publishScreen(stream: MediaStream) {
    const voice = active.current;
    if (!voice || !connected) { stream.getTracks().forEach(track => track.stop()); return; }
    try {
      const { Track } = await import('livekit-client');
      for (const track of stream.getTracks()) { if (active.current?.sessionId !== voice.sessionId) { stream.getTracks().forEach(track => track.stop()); return; } await voice.live.localParticipant.publishTrack(track, {source:track.kind === 'video' ? Track.Source.ScreenShare : Track.Source.ScreenShareAudio}); }
    } catch (error) { stream.getTracks().forEach(track => track.stop()); latest.current.report(error); }
  }
  async function stopScreen() { try { await active.current?.live.localParticipant.setScreenShareEnabled(false); } catch (error) { latest.current.report(error); } }
  return {roomId,connected,busy,micMuted,cameraOn,shares,participants,join,leave,mic,camera,listen,publishScreen,stopScreen};
}
