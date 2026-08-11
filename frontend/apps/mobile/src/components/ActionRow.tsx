import { Check, Clock3 } from 'lucide-react-native';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';
import { Action } from '../types';

type Props = {
  action: Action;
  journeyTitle?: string;
  onComplete: (id: string) => void;
};

export function ActionRow({ action, journeyTitle, onComplete }: Props) {
  const completed = action.state === 'completed';
  return (
    <View style={[styles.row, completed && styles.completedRow]}>
      <Pressable
        accessibilityLabel={completed ? '已完成' : `完成${action.title}`}
        accessibilityRole="checkbox"
        accessibilityState={{ checked: completed }}
        disabled={completed}
        hitSlop={8}
        onPress={() => onComplete(action.id)}
        style={[styles.check, completed && styles.checked]}
      >
        {completed ? <Check color={colors.surface} size={15} strokeWidth={3} /> : null}
      </Pressable>
      <View style={styles.content}>
        <View style={styles.topline}>
          <Text numberOfLines={1} style={[styles.title, completed && styles.completedText]}>
            {action.title}
          </Text>
          <View style={styles.duration}>
            <Clock3 color={colors.faint} size={13} />
            <Text style={styles.durationText}>{action.estimated_minutes} 分钟</Text>
          </View>
        </View>
        <Text numberOfLines={2} style={styles.detail}>
          {action.detail}
        </Text>
        <Text style={styles.journey}>{journeyTitle ?? action.scheduled_label}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    minHeight: 108,
    flexDirection: 'row',
    gap: 13,
    paddingHorizontal: 16,
    paddingVertical: 16,
    backgroundColor: colors.surface,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: colors.line,
  },
  completedRow: { opacity: 0.72 },
  check: {
    width: 24,
    height: 24,
    marginTop: 2,
    borderRadius: 6,
    borderWidth: 1.5,
    borderColor: colors.evergreen,
    alignItems: 'center',
    justifyContent: 'center',
  },
  checked: { backgroundColor: colors.evergreen },
  content: { flex: 1, minWidth: 0 },
  topline: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10 },
  title: { flex: 1, color: colors.ink, fontSize: 16, lineHeight: 22, fontWeight: '700', letterSpacing: 0 },
  completedText: { textDecorationLine: 'line-through', color: colors.muted },
  duration: { flexDirection: 'row', alignItems: 'center', gap: 4 },
  durationText: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  detail: { color: colors.muted, fontSize: 13, lineHeight: 19, marginTop: 5, letterSpacing: 0 },
  journey: { color: colors.evergreen, fontSize: 11, fontWeight: '600', marginTop: 7, letterSpacing: 0 },
});

