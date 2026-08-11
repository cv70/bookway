import { AppState, type AppStateStatus } from 'react-native';

import { sendEvents } from '../api/client';

type EventType =
  | 'impression'
  | 'click'
  | 'view'
  | 'like'
  | 'bookmark'
  | 'share'
  | 'hide'
  | 'complete'
  | 'follow'
  | 'search_submit';

type PendingEvent = {
  event_id: string;
  event_type: EventType;
  session_id: string;
  request_id?: string;
  component_id: string;
  content_id?: string;
  position?: number;
  occurred_at: string;
  source: string;
};

const sessionId = uuid();
const pending: PendingEvent[] = [];
const seenImpressions = new Set<string>();
let flushTimer: ReturnType<typeof setTimeout> | undefined;
let appStateSubscription: { remove: () => void } | undefined;
let flushing = false;

export const eventReporter = {
  start() {
    if (appStateSubscription) return;
    appStateSubscription = AppState.addEventListener('change', (state: AppStateStatus) => {
      if (state === 'background' || state === 'inactive') void flush();
    });
  },
  stop() {
    appStateSubscription?.remove();
    appStateSubscription = undefined;
    void flush();
  },
  track(input: Omit<PendingEvent, 'event_id' | 'session_id' | 'occurred_at' | 'source'> & { source?: string }) {
    pending.push({ ...input, event_id: uuid(), session_id: sessionId, occurred_at: new Date().toISOString(), source: input.source ?? 'mobile' });
    if (pending.length >= 100) void flush();
    else scheduleFlush();
  },
  impression(contentId: string, componentId: string, position?: number) {
    const key = `${sessionId}:${componentId}:${contentId}`;
    if (seenImpressions.has(key)) return;
    seenImpressions.add(key);
    this.track({ event_type: 'impression', component_id: componentId, content_id: contentId, position });
  },
  flush,
};

function scheduleFlush() {
  if (flushTimer) return;
  flushTimer = setTimeout(() => { flushTimer = undefined; void flush(); }, 2000);
}

async function flush() {
  if (flushing || pending.length === 0) return;
  flushing = true;
  const batch = pending.splice(0, 100);
  try { await sendEvents(batch); } catch {
    pending.unshift(...batch);
    if (pending.length > 1000) pending.splice(0, pending.length - 1000);
    scheduleFlush();
  } finally { flushing = false; }
}

function uuid() {
  const bytes = Array.from({ length: 16 }, () => Math.floor(Math.random() * 256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
