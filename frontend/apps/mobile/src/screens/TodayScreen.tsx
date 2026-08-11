import { Flame, TimerReset } from 'lucide-react-native';
import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { ActionRow } from '../components/ActionRow';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';
import { Journey, Today } from '../types';

type Props = {
  today: Today;
  journeys: Journey[];
  onComplete: (id: string) => void;
};

export function TodayScreen({ today, journeys, onComplete }: Props) {
  const progress = today.total === 0 ? 0 : Math.round((today.completed / today.total) * 100);
  const journeyNames = new Map(journeys.map((journey) => [journey.id, journey.title]));

  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader action="bell" eyebrow="8 月 11 日 · 星期二" title="今天，走一点" />
      <View style={styles.summary}>
        <View style={styles.ring}>
          <Text style={styles.percent}>{progress}%</Text>
          <Text style={styles.ringLabel}>今日</Text>
        </View>
        <View style={styles.summaryCopy}>
          <Text style={styles.summaryTitle}>{today.completed} / {today.total} 项已完成</Text>
          <Text style={styles.summaryText}>每一步都算数，按自己的节奏继续。</Text>
          <View style={styles.metrics}>
            <View style={styles.metric}>
              <TimerReset color={colors.evergreenSoft} size={16} />
              <Text style={styles.metricText}>专注 {today.focus_minutes} 分钟</Text>
            </View>
            <View style={styles.metric}>
              <Flame color={colors.goldSoft} size={16} fill={colors.goldSoft} />
              <Text style={styles.metricText}>连续 6 天</Text>
            </View>
          </View>
        </View>
      </View>
      <View style={styles.sectionHeading}>
        <Text style={styles.sectionTitle}>今日行动</Text>
        <Text style={styles.sectionMeta}>{today.actions.length} 项</Text>
      </View>
      <View style={styles.actionList}>
        {today.actions.map((action) => (
          <ActionRow
            action={action}
            journeyTitle={journeyNames.get(action.journey_id)}
            key={action.id}
            onComplete={onComplete}
          />
        ))}
      </View>
      <View style={styles.reflect}>
        <Text style={styles.reflectLabel}>今日一问</Text>
        <Text style={styles.reflectText}>今天哪一个瞬间，让你觉得自己正在变好？</Text>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 28 },
  summary: { minHeight: 150, backgroundColor: colors.evergreen, paddingHorizontal: 20, paddingVertical: 22, flexDirection: 'row', alignItems: 'center', gap: 18 },
  ring: { width: 78, height: 78, borderRadius: 39, borderWidth: 5, borderColor: '#79A18F', alignItems: 'center', justifyContent: 'center' },
  percent: { color: colors.surface, fontSize: 20, lineHeight: 25, fontWeight: '800', letterSpacing: 0 },
  ringLabel: { color: '#BFD4CB', fontSize: 10, lineHeight: 14, letterSpacing: 0 },
  summaryCopy: { flex: 1, minWidth: 0 },
  summaryTitle: { color: colors.surface, fontSize: 18, lineHeight: 25, fontWeight: '700', letterSpacing: 0 },
  summaryText: { color: '#D7E4DE', fontSize: 12, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  metrics: { flexDirection: 'row', flexWrap: 'wrap', gap: 12, marginTop: 13 },
  metric: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  metricText: { color: '#E9F0ED', fontSize: 10, fontWeight: '600', letterSpacing: 0 },
  sectionHeading: { height: 58, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  sectionTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  sectionMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  actionList: { borderTopColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth },
  reflect: { marginTop: 16, paddingHorizontal: 20, paddingVertical: 19, backgroundColor: colors.goldSoft, borderLeftWidth: 4, borderLeftColor: colors.gold },
  reflectLabel: { color: colors.gold, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  reflectText: { color: colors.ink, fontSize: 15, lineHeight: 23, fontWeight: '600', marginTop: 6, letterSpacing: 0 },
});

