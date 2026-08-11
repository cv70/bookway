import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { JourneyCard } from '../components/JourneyCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';
import { Journey } from '../types';

type Props = { journeys: Journey[]; onCreate: () => void };

export function JourneysScreen({ journeys, onCreate }: Props) {
  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader action="plus" onAction={onCreate} title="我的路线" />
      <View style={styles.overview}>
        <Stat label="进行中" value={journeys.filter((item) => item.status === 'active').length} />
        <View style={styles.divider} />
        <Stat label="累计行动" value={42} />
        <View style={styles.divider} />
        <Stat label="本月专注" value="9.6h" />
      </View>
      <View style={styles.heading}>
        <Text style={styles.headingText}>正在走</Text>
        <Text style={styles.headingMeta}>按最近行动排序</Text>
      </View>
      <View style={styles.list}>
        {journeys.map((journey) => <JourneyCard journey={journey} key={journey.id} />)}
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
  list: { paddingHorizontal: 16, gap: 12 },
});

