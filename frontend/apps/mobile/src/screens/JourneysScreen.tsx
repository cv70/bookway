import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useState } from 'react';

import { JourneyCard } from '../components/JourneyCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';
import { Journey } from '../types';

type Props = { journeys: Journey[]; onCreate: () => void; onOpen: (journey: Journey) => void };
type Filter = 'active' | 'paused' | 'completed' | 'all';

export function JourneysScreen({ journeys, onCreate, onOpen }: Props) {
  const [filter, setFilter] = useState<Filter>('active');
  const activeCount = journeys.filter((item) => item.status === 'active').length;
  const averageProgress = journeys.length
    ? Math.round(journeys.reduce((total, journey) => total + journey.progress, 0) / journeys.length)
    : 0;
  const participantCount = journeys.reduce((total, journey) => total + journey.participant_count, 0);
  const visibleJourneys = filter === 'all' ? journeys : journeys.filter((journey) => journey.status === filter);

  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader action="plus" onAction={onCreate} title="我的路线" />
      <View style={styles.overview}>
        <Stat label="进行中" value={activeCount} />
        <View style={styles.divider} />
        <Stat label="平均进度" value={`${averageProgress}%`} />
        <View style={styles.divider} />
        <Stat label="同行人数" value={participantCount.toLocaleString()} />
      </View>
      <View style={styles.heading}>
        <Text style={styles.headingText}>正在走</Text>
        <Text style={styles.headingMeta}>按最近行动排序</Text>
      </View>
      <View style={styles.filters}>{([['active', '进行中'], ['paused', '暂停'], ['completed', '完成'], ['all', '全部']] as const).map(([key, label]) => <Pressable accessibilityRole="tab" accessibilityState={{ selected: filter === key }} key={key} onPress={() => setFilter(key)} style={[styles.filter, filter === key && styles.filterSelected]}><Text style={[styles.filterText, filter === key && styles.filterTextSelected]}>{label}</Text></Pressable>)}</View>
      <View style={styles.list}>
        {visibleJourneys.length ? visibleJourneys.map((journey) => <JourneyCard journey={journey} key={journey.id} onPress={() => onOpen(journey)} />) : <Text style={styles.empty}>还没有符合条件的路线</Text>}
      </View>
    </ScrollView>
  );
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <View style={styles.stat}>
      <Text style={styles.statValue}>{value}</Text>
      <Text style={styles.statLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 30 },
  overview: { minHeight: 92, flexDirection: 'row', alignItems: 'center', backgroundColor: colors.surface, borderTopColor: colors.line, borderBottomColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth, borderBottomWidth: StyleSheet.hairlineWidth },
  stat: { flex: 1, alignItems: 'center' },
  statValue: { color: colors.ink, fontSize: 20, lineHeight: 26, fontWeight: '800', letterSpacing: 0 },
  statLabel: { color: colors.faint, fontSize: 10, marginTop: 3, letterSpacing: 0 },
  divider: { width: StyleSheet.hairlineWidth, height: 34, backgroundColor: colors.line },
  heading: { height: 62, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  headingText: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  headingMeta: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  filters: { paddingHorizontal: 16, paddingBottom: 14, flexDirection: 'row', gap: 7 },
  filter: { height: 32, paddingHorizontal: 12, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  filterSelected: { borderColor: colors.ink, backgroundColor: colors.ink },
  filterText: { color: colors.muted, fontSize: 11, fontWeight: '600', letterSpacing: 0 },
  filterTextSelected: { color: colors.surface },
  list: { paddingHorizontal: 16, gap: 12 },
  empty: { paddingVertical: 25, textAlign: 'center', color: colors.faint, fontSize: 13, letterSpacing: 0 },
});
