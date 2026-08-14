import { CalendarClock, Check, Clock3, NotebookPen, Pause, Play, SkipForward, X } from 'lucide-react-native';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import {
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { colors } from '../theme';
import { Action, ActionUpdate } from '../types';

type Props = {
  action?: Action;
  journeyTitle?: string;
  visible: boolean;
  onClose: () => void;
  onComplete: (actionId: string) => void;
  onUpdate: (actionId: string, updates: ActionUpdate) => void;
  onCreateEntry: (action: Action) => void;
};

export function ActionDetailModal({
  action,
  journeyTitle,
  visible,
  onClose,
  onComplete,
  onUpdate,
  onCreateEntry,
}: Props) {
  const [elapsed, setElapsed] = useState(0);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    if (!visible) return;
    setElapsed(0);
    setRunning(false);
  }, [action?.id, visible]);

  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => clearInterval(timer);
  }, [running]);

  const elapsedLabel = useMemo(() => formatElapsed(elapsed), [elapsed]);
  if (!action) return null;
  const finished = action.state === 'completed';

  const complete = () => {
    setRunning(false);
    onComplete(action.id);
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭行动详情" hitSlop={10} onPress={onClose} style={styles.iconButton}>
            <X color={colors.ink} size={22} />
          </Pressable>
          <Text numberOfLines={1} style={styles.headerTitle}>行动详情</Text>
          <View style={styles.iconButton} />
        </View>
        <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
          <Text style={styles.route}>{journeyTitle ?? action.scheduled_label}</Text>
          <Text style={styles.title}>{action.title}</Text>
          <Text style={styles.detail}>{action.detail || '为这一步留下你的标准或提醒。'}</Text>

          <View style={styles.metaRow}>
            <Meta icon={<Clock3 color={colors.evergreen} size={17} />} label={`预计 ${action.estimated_minutes} 分钟`} />
            <Meta icon={<CalendarClock color={colors.gold} size={17} />} label={action.scheduled_label} />
          </View>

          <View style={styles.timerPanel}>
            <Text style={styles.timerCaption}>{running ? '正在进行' : finished ? '已完成' : '专注计时'}</Text>
            <Text style={styles.timer}>{elapsedLabel}</Text>
            {!finished ? (
              <Pressable
                accessibilityLabel={running ? '暂停计时' : '开始计时'}
                onPress={() => setRunning((value) => !value)}
                style={({ pressed }) => [styles.timerButton, pressed && styles.pressed]}
              >
                {running ? <Pause color={colors.surface} size={18} fill={colors.surface} /> : <Play color={colors.surface} size={18} fill={colors.surface} />}
                <Text style={styles.timerButtonText}>{running ? '暂停' : '开始计时'}</Text>
              </Pressable>
            ) : null}
          </View>

          {!finished ? (
            <View style={styles.actionGroup}>
              <Pressable
                accessibilityRole="button"
                onPress={complete}
                style={({ pressed }) => [styles.primaryAction, pressed && styles.pressed]}
              >
                <Check color={colors.surface} size={19} strokeWidth={3} />
                <Text style={styles.primaryText}>完成行动</Text>
              </Pressable>
              <View style={styles.secondaryActions}>
                <Pressable
                  accessibilityLabel="跳过行动"
                  onPress={() => {
                    onUpdate(action.id, { state: 'skipped' });
                    onClose();
                  }}
                  style={({ pressed }) => [styles.secondaryAction, pressed && styles.pressed]}
                >
                  <SkipForward color={colors.muted} size={18} />
                  <Text style={styles.secondaryText}>跳过</Text>
                </Pressable>
                <Pressable
                  accessibilityLabel="改到明天"
                  onPress={() => {
                    onUpdate(action.id, { scheduled_label: '明天' });
                    onClose();
                  }}
                  style={({ pressed }) => [styles.secondaryAction, pressed && styles.pressed]}
                >
                  <CalendarClock color={colors.muted} size={18} />
                  <Text style={styles.secondaryText}>改到明天</Text>
                </Pressable>
              </View>
            </View>
          ) : null}

          <Pressable
            accessibilityRole="button"
            onPress={() => onCreateEntry(action)}
            style={({ pressed }) => [styles.entryAction, pressed && styles.pressed]}
          >
            <NotebookPen color={colors.evergreen} size={19} />
            <View style={styles.entryCopy}>
              <Text style={styles.entryTitle}>记录这一刻</Text>
              <Text style={styles.entryText}>留下感受、照片、地点或数值</Text>
            </View>
          </Pressable>
        </ScrollView>
      </View>
    </Modal>
  );
}

function Meta({ icon, label }: { icon: ReactNode; label: string }) {
  return <View style={styles.meta}>{icon}<Text style={styles.metaText}>{label}</Text></View>;
}

function formatElapsed(totalSeconds: number) {
  const minutes = Math.floor(totalSeconds / 60).toString().padStart(2, '0');
  const seconds = (totalSeconds % 60).toString().padStart(2, '0');
  return `${minutes}:${seconds}`;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  content: { padding: 20, paddingBottom: 40 },
  route: { color: colors.evergreen, fontSize: 12, lineHeight: 18, fontWeight: '700', letterSpacing: 0 },
  title: { color: colors.ink, fontSize: 26, lineHeight: 35, fontWeight: '700', marginTop: 8, letterSpacing: 0 },
  detail: { color: colors.muted, fontSize: 14, lineHeight: 22, marginTop: 8, letterSpacing: 0 },
  metaRow: { flexDirection: 'row', flexWrap: 'wrap', gap: 14, marginTop: 20 },
  meta: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  metaText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  timerPanel: { alignItems: 'center', marginTop: 30, paddingVertical: 28, backgroundColor: colors.evergreenSoft, borderRadius: 8 },
  timerCaption: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  timer: { color: colors.ink, fontSize: 42, lineHeight: 52, fontWeight: '800', marginTop: 4, fontVariant: ['tabular-nums'], letterSpacing: 0 },
  timerButton: { height: 44, minWidth: 128, marginTop: 15, paddingHorizontal: 17, borderRadius: 6, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, backgroundColor: colors.evergreen },
  timerButtonText: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  actionGroup: { marginTop: 26, gap: 10 },
  primaryAction: { height: 52, borderRadius: 6, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, backgroundColor: colors.evergreen },
  primaryText: { color: colors.surface, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  secondaryActions: { flexDirection: 'row', gap: 10 },
  secondaryAction: { flex: 1, height: 46, borderRadius: 6, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  secondaryText: { color: colors.muted, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  entryAction: { minHeight: 70, marginTop: 28, padding: 14, borderRadius: 7, flexDirection: 'row', alignItems: 'center', gap: 12, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  entryCopy: { flex: 1, minWidth: 0 },
  entryTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  entryText: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 3, letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
