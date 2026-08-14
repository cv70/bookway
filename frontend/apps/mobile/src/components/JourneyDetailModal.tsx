import { CheckCircle2, Clock3, Pause, Play, Plus, Share2, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { colors, domainMeta } from '../theme';
import { Action, CreateActionInput, Journey } from '../types';
import { DomainBadge } from './DomainBadge';

type Props = {
  journey?: Journey;
  actions: Action[];
  visible: boolean;
  onClose: () => void;
  onOpenAction: (action: Action) => void;
  onUpdateJourney: (journeyId: string, updates: Partial<Journey>) => void;
  onAddAction: (journeyId: string, input: CreateActionInput) => void;
  onPublish: (journey: Journey) => void;
};

export function JourneyDetailModal({
  journey,
  actions,
  visible,
  onClose,
  onOpenAction,
  onUpdateJourney,
  onAddAction,
  onPublish,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState('');
  const [detail, setDetail] = useState('');
  const [minutes, setMinutes] = useState('20');

  useEffect(() => {
    if (!visible) return;
    setAdding(false);
    setTitle('');
    setDetail('');
    setMinutes('20');
  }, [journey?.id, visible]);

  if (!journey) return null;
  const meta = domainMeta[journey.domain];
  const pending = actions.filter((action) => action.state === 'pending');
  const done = actions.filter((action) => action.state === 'completed');
  const paused = journey.status === 'paused';
  const completed = journey.status === 'completed';
  const canAdd = title.trim().length > 0 && Number(minutes) > 0;

  const addAction = () => {
    if (!canAdd) return;
    onAddAction(journey.id, {
      title: title.trim(),
      detail: detail.trim(),
      estimated_minutes: Math.min(720, Math.max(1, Number(minutes))),
      scheduled_label: '今天',
    });
    setAdding(false);
    setTitle('');
    setDetail('');
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭路线详情" hitSlop={10} onPress={onClose} style={styles.iconButton}>
            <X color={colors.ink} size={22} />
          </Pressable>
          <Text numberOfLines={1} style={styles.headerTitle}>路线详情</Text>
          <View style={styles.iconButton} />
        </View>
        <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
          <View style={styles.topline}>
            <DomainBadge domain={journey.domain} />
            <Text style={[styles.status, { color: completed ? colors.evergreen : paused ? colors.gold : meta.color }]}>
              {completed ? '已完成' : paused ? '已暂停' : '进行中'}
            </Text>
          </View>
          <Text style={styles.title}>{journey.title}</Text>
          <Text style={styles.intent}>{journey.intent || '为自己留下一条可以慢慢走的路。'}</Text>

          <View style={styles.progressCard}>
            <View style={styles.progressLine}>
              <Text style={styles.progressTitle}>路线进度</Text>
              <Text style={styles.progressValue}>{journey.progress}%</Text>
            </View>
            <View style={styles.track}>
              <View style={[styles.fill, { width: `${journey.progress}%`, backgroundColor: meta.color }]} />
            </View>
            <Text style={styles.progressMeta}>{journey.duration_label} · 已完成 {done.length} 项行动</Text>
          </View>

          {!completed ? (
            <View style={styles.routeActions}>
              <Pressable
                accessibilityLabel={paused ? '恢复路线' : '暂停路线'}
                onPress={() => onUpdateJourney(journey.id, { status: paused ? 'active' : 'paused' })}
                style={({ pressed }) => [styles.smallAction, pressed && styles.pressed]}
              >
                {paused ? <Play color={colors.evergreen} size={17} /> : <Pause color={colors.gold} size={17} />}
                <Text style={styles.smallActionText}>{paused ? '恢复' : '暂停'}</Text>
              </Pressable>
              <Pressable
                accessibilityLabel="完成路线"
                onPress={() => onUpdateJourney(journey.id, { status: 'completed', progress: 100 })}
                style={({ pressed }) => [styles.smallAction, pressed && styles.pressed]}
              >
                <CheckCircle2 color={colors.evergreen} size={17} />
                <Text style={styles.smallActionText}>完成路线</Text>
              </Pressable>
              <Pressable
                accessibilityLabel="发布路线"
                onPress={() => onPublish(journey)}
                style={({ pressed }) => [styles.smallAction, pressed && styles.pressed]}
              >
                <Share2 color={colors.blue} size={17} />
                <Text style={styles.smallActionText}>发布路线</Text>
              </Pressable>
            </View>
          ) : null}

          <View style={styles.sectionHeader}>
            <Text style={styles.sectionTitle}>行动安排</Text>
            {!completed ? (
              <Pressable accessibilityLabel="新增行动" hitSlop={8} onPress={() => setAdding((value) => !value)} style={styles.addButton}>
                <Plus color={colors.evergreen} size={19} />
              </Pressable>
            ) : null}
          </View>
          {adding ? (
            <View style={styles.addForm}>
              <TextInput onChangeText={setTitle} placeholder="行动名称" placeholderTextColor={colors.faint} style={styles.input} value={title} />
              <TextInput onChangeText={setDetail} placeholder="备注或完成标准（可选）" placeholderTextColor={colors.faint} style={styles.input} value={detail} />
              <View style={styles.addFooter}>
                <View style={styles.minutesInput}><Clock3 color={colors.faint} size={16} /><TextInput keyboardType="number-pad" onChangeText={setMinutes} style={styles.minutesText} value={minutes} /><Text style={styles.minutesUnit}>分钟</Text></View>
                <Pressable disabled={!canAdd} onPress={addAction} style={[styles.saveButton, !canAdd && styles.disabled]}><Text style={styles.saveText}>加入安排</Text></Pressable>
              </View>
            </View>
          ) : null}
          <View style={styles.actionList}>
            {pending.length === 0 ? <Text style={styles.empty}>还没有待完成的行动</Text> : pending.map((action) => <ActionItem action={action} key={action.id} onPress={() => onOpenAction(action)} />)}
          </View>
          {done.length > 0 ? (
            <View style={styles.completedSection}>
              <Text style={styles.completedTitle}>已完成 · {done.length}</Text>
              {done.map((action) => <ActionItem action={action} key={action.id} onPress={() => onOpenAction(action)} />)}
            </View>
          ) : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

function ActionItem({ action, onPress }: { action: Action; onPress: () => void }) {
  const done = action.state === 'completed';
  return (
    <Pressable onPress={onPress} style={({ pressed }) => [styles.actionItem, pressed && styles.pressed]}>
      <View style={[styles.actionState, done && styles.actionStateDone]}>{done ? <CheckCircle2 color={colors.surface} size={15} /> : null}</View>
      <View style={styles.actionCopy}>
        <Text numberOfLines={1} style={[styles.actionTitle, done && styles.actionTitleDone]}>{action.title}</Text>
        <Text numberOfLines={1} style={styles.actionMeta}>{action.scheduled_label} · {action.estimated_minutes} 分钟</Text>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  content: { padding: 20, paddingBottom: 42 },
  topline: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  status: { fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  title: { color: colors.ink, fontSize: 26, lineHeight: 35, fontWeight: '700', marginTop: 13, letterSpacing: 0 },
  intent: { color: colors.muted, fontSize: 14, lineHeight: 22, marginTop: 7, letterSpacing: 0 },
  progressCard: { marginTop: 23, padding: 16, borderRadius: 8, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  progressLine: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'baseline' },
  progressTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  progressValue: { color: colors.ink, fontSize: 22, fontWeight: '800', letterSpacing: 0 },
  track: { height: 6, marginTop: 12, overflow: 'hidden', borderRadius: 3, backgroundColor: colors.line },
  fill: { height: 6, borderRadius: 3 },
  progressMeta: { color: colors.faint, fontSize: 11, marginTop: 9, letterSpacing: 0 },
  routeActions: { flexDirection: 'row', gap: 10, marginTop: 12 },
  smallAction: { flex: 1, height: 42, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  smallActionText: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  sectionHeader: { height: 64, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 16 },
  sectionTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  addButton: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center', borderRadius: 6, backgroundColor: colors.evergreenSoft },
  addForm: { padding: 13, gap: 9, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line, marginBottom: 10 },
  input: { minHeight: 42, borderRadius: 5, borderWidth: 1, borderColor: colors.line, paddingHorizontal: 10, color: colors.ink, backgroundColor: colors.background, fontSize: 13, letterSpacing: 0 },
  addFooter: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10 },
  minutesInput: { flex: 1, height: 40, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', gap: 6, borderRadius: 5, backgroundColor: colors.background },
  minutesText: { width: 28, paddingVertical: 0, color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  minutesUnit: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  saveButton: { height: 40, paddingHorizontal: 13, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  saveText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.4 },
  actionList: { gap: 8 },
  empty: { paddingVertical: 20, textAlign: 'center', color: colors.faint, fontSize: 13, letterSpacing: 0 },
  actionItem: { minHeight: 63, paddingHorizontal: 13, flexDirection: 'row', alignItems: 'center', gap: 11, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  actionState: { width: 21, height: 21, borderRadius: 6, borderWidth: 1.5, borderColor: colors.evergreen, alignItems: 'center', justifyContent: 'center' },
  actionStateDone: { backgroundColor: colors.evergreen },
  actionCopy: { flex: 1, minWidth: 0 },
  actionTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  actionTitleDone: { color: colors.muted, textDecorationLine: 'line-through' },
  actionMeta: { color: colors.faint, fontSize: 11, marginTop: 3, letterSpacing: 0 },
  completedSection: { marginTop: 23 },
  completedTitle: { color: colors.muted, fontSize: 12, fontWeight: '700', marginBottom: 8, letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
