import { useCallback, useEffect, useRef, useState } from "react";

export function useScreenShare(channelId: string, roomName: string) {
  const [stream, setStream] = useState<MediaStream | null>(null);
  const active = useRef<MediaStream | null>(null);
  const stop = useCallback(() => {
    const previous = active.current;
    active.current = null;
    previous?.getTracks().forEach(track => track.stop());
    setStream(null);
  }, []);

  const start = useCallback((next: MediaStream) => {
    active.current?.getTracks().forEach(track => track.stop());
    active.current = next;
    const track = next.getVideoTracks()[0];
    if (!track || track.readyState === "ended") {
      next.getTracks().forEach(item => item.stop());
      active.current = null;
      setStream(null);
      return;
    }
    track.addEventListener("ended", () => {
      if (active.current === next) stop();
    }, { once: true });
    setStream(next);
  }, [stop]);

  useEffect(() => {
    stop();
    return () => {
      active.current?.getTracks().forEach(track => track.stop());
      active.current = null;
    };
  }, [channelId, roomName, stop]);

  return { stream, start, stop };
}
