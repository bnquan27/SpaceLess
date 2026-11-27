import { useEffect, useRef, useState } from "react";

const windowMs = 2_000;
const maxMessages = 5;

export const messageRateWarning = "NÀY BÌNH TĨNH, BẠN ĐANG GỬI TIN NHẮN QUÁ NHANH";

export function useMessageSendLimit() {
  const sentAt = useRef<number[]>([]);
  const [retryAt, setRetryAt] = useState<number | null>(null);

  useEffect(() => {
    if (retryAt === null) return;
    const timer = window.setTimeout(() => setRetryAt(null), Math.ceil(retryAt - performance.now()));
    return () => window.clearTimeout(timer);
  }, [retryAt]);

  function trySend() {
    const now = performance.now();
    const recent = sentAt.current.filter((timestamp) => timestamp > now - windowMs);
    if (recent.length >= maxMessages) {
      setRetryAt(recent[0] + windowMs);
      return false;
    }
    sentAt.current = [...recent, now];
    setRetryAt(null);
    return true;
  }

  return { trySend, blocked: retryAt !== null };
}

export type MessageSendLimit = ReturnType<typeof useMessageSendLimit>;
