export type ScheduleDay = 'today' | 'tomorrow';

export type ActionSchedule = {
  scheduled_label: string;
  scheduled_for: string;
  scheduled_timezone: string;
};

export function defaultScheduleTime(): string {
  return '19:00';
}

export function scheduleForDay(day: ScheduleDay, time: string): ActionSchedule | null {
  const normalizedTime = time.trim();
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(normalizedTime)) return null;

  const [hours, minutes] = normalizedTime.split(':').map(Number);
  const scheduled = new Date();
  if (day === 'tomorrow') scheduled.setDate(scheduled.getDate() + 1);
  scheduled.setHours(hours, minutes, 0, 0);

  return {
    scheduled_label: `${day === 'today' ? '今天' : '明天'} ${normalizedTime}`,
    scheduled_for: formatLocalRfc3339(scheduled),
    scheduled_timezone: localTimezone(),
  };
}

export function tomorrowScheduleFrom(timestamp?: string): ActionSchedule {
  const time = timestamp?.match(/T(\d{2}:\d{2})/)?.[1] ?? defaultScheduleTime();
  return scheduleForDay('tomorrow', time) ?? scheduleForDay('tomorrow', defaultScheduleTime())!;
}

export function localScheduleContext(): { date: string; timezone: string } {
  const now = new Date();
  return { date: formatLocalDate(now), timezone: localTimezone() };
}

function localTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

function formatLocalRfc3339(value: Date): string {
  const offsetMinutes = -value.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? '+' : '-';
  const absoluteOffset = Math.abs(offsetMinutes);
  const offsetHours = Math.floor(absoluteOffset / 60).toString().padStart(2, '0');
  const offsetRemainder = (absoluteOffset % 60).toString().padStart(2, '0');
  return `${formatLocalDate(value)}T${value.getHours().toString().padStart(2, '0')}:${value.getMinutes().toString().padStart(2, '0')}:00${sign}${offsetHours}:${offsetRemainder}`;
}

function formatLocalDate(value: Date): string {
  const year = value.getFullYear().toString().padStart(4, '0');
  const month = (value.getMonth() + 1).toString().padStart(2, '0');
  const day = value.getDate().toString().padStart(2, '0');
  return `${year}-${month}-${day}`;
}
