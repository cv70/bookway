import { ChevronRight, Clock3, UsersRound } from 'lucide-react-native';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { colors, domainMeta } from '../theme';
import { Journey } from '../types';
import { DomainBadge } from './DomainBadge';

export function JourneyCard({ journey }: { journey: Journey }) {
  const meta = domainMeta[journey.domain];
  return (
    <Pressable style={({ pressed }) => [styles.card, pressed && styles.pressed]}>
      <View style={styles.header}>
        <DomainBadge domain={journey.domain} />
        <View style={styles.duration}>
          <Clock3 color={colors.faint} size={13} />
          <Text style={styles.meta}>{journey.duration_label}</Text>
        </View>
      </View>
      <Text style={styles.title}>{journey.title}</Text>
      <Text numberOfLines={2} style={styles.intent}>{journey.intent}</Text>
      <View style={styles.progressTrack}>
        <View style={[styles.progressFill, { backgroundColor: meta.color, width: `${journey.progress}%` }]} />
      </View>
      <View style={styles.progressLine}>
        <Text style={styles.progressText}>{journey.progress}%</Text>
        <View style={styles.people}>
          <UsersRound color={colors.faint} size={14} />
          <Text style={styles.meta}>{journey.participant_count.toLocaleString()} 人同行</Text>
        </View>
      </View>
      <View style={styles.next}>
        <View style={styles.nextCopy}>
          <Text style={styles.nextLabel}>下一步</Text>
          <Text numberOfLines={1} style={styles.nextTitle}>{journey.next_action}</Text>
        </View>
        <ChevronRight color={colors.muted} size={18} />
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  card: { backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line, borderRadius: 8, padding: 16 },
  pressed: { opacity: 0.68 },
  header: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  duration: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  meta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  title: { color: colors.ink, fontSize: 18, lineHeight: 25, fontWeight: '700', marginTop: 13, letterSpacing: 0 },
  intent: { color: colors.muted, fontSize: 13, lineHeight: 20, marginTop: 5, letterSpacing: 0 },
  progressTrack: { height: 5, borderRadius: 3, backgroundColor: colors.line, marginTop: 16, overflow: 'hidden' },
  progressFill: { height: 5, borderRadius: 3 },
  progressLine: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginTop: 7 },
  progressText: { color: colors.ink, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  people: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  next: { flexDirection: 'row', alignItems: 'center', marginTop: 15, paddingTop: 13, borderTopColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth },
  nextCopy: { flex: 1, minWidth: 0 },
  nextLabel: { color: colors.faint, fontSize: 10, fontWeight: '600', letterSpacing: 0 },
  nextTitle: { color: colors.ink, fontSize: 13, lineHeight: 19, fontWeight: '600', marginTop: 2, letterSpacing: 0 },
});

