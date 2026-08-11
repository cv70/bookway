import { GrowthDomain } from './types';

export const colors = {
  ink: '#202522',
  muted: '#6D746F',
  faint: '#9AA19C',
  background: '#F7F8F5',
  surface: '#FFFFFF',
  line: '#E3E7E2',
  evergreen: '#245746',
  evergreenSoft: '#E3EFEA',
  coral: '#D95D45',
  coralSoft: '#F8E7E2',
  blue: '#39759B',
  blueSoft: '#E5EFF5',
  gold: '#A66F13',
  goldSoft: '#F6ECD9',
  plum: '#77506D',
  plumSoft: '#F0E8EE',
};

export const domainMeta: Record<
  GrowthDomain,
  { label: string; color: string; background: string }
> = {
  learning: { label: '学习', color: colors.blue, background: colors.blueSoft },
  movement: { label: '运动', color: colors.coral, background: colors.coralSoft },
  wellness: { label: '健康', color: colors.evergreen, background: colors.evergreenSoft },
  travel: { label: '旅行', color: colors.gold, background: colors.goldSoft },
  leisure: { label: '休闲', color: colors.plum, background: colors.plumSoft },
};

