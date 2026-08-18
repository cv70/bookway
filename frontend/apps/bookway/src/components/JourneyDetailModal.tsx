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
import { Action, CreateActionInput, Journey, JourneyType, Weekday } from '../types';
import { defaultScheduleTime, scheduleForDay, ScheduleDay } from '../utils/scheduling';
import { DomainBadge } from './DomainBadge';

const weekdayByIndex: Weekday[] = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];
const journeyTypeLabels: Record<JourneyType, string> = {
  habit: '习惯型',
  project: '项目型',
  quantity: '数量型',
  travel: '旅程型',
  challenge: '挑战型',
};

type Props = {
  journey?: Journey;
  actions: Action[];
  visible: boolean;
  onClose: () => void;
  onOpenAction: (action: Action) => void;
  onUpdateJourney: (journeyId: string, updates: Partial<Journey>) => void;
  onAddAction: (journeyId: string, input: CreateActionInput) => void;
  onPublish: (journey: Journey, actions: Action[]) => void;
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
  const [scheduleDay, setScheduleDay] = useState<ScheduleDay>('today');
  const [scheduleTime, setScheduleTime] = useState(defaultScheduleTime());
  const [stageId, setStageId] = useState<string>();
  const [repeat, setRepeat] = useState<'none' | 'daily' | 'weekly'>('none');

  useEffect(() => {
    if (!visible) return;
    setAdding(false);
    setTitle('');
    setDetail('');
    setMinutes('20');
    setScheduleDay('today');
    setScheduleTime(defaultScheduleTime());
    setStageId(undefined);
    setRepeat('none');
  }, [journey?.id, visible]);

  if (!journey) return null;
  const meta = domainMeta[journey.domain];
  const pending = actions.filter((action) => action.state === 'pending');
  const done = actions.filter((action) => action.state === 'completed');
  const paused = journey.status === 'paused';
  const completed = journey.status === 'completed';
  const schedule = scheduleForDay(scheduleDay, scheduleTime);
  const canAdd = title.trim().length > 0 && Number(minutes) > 0 && schedule !== null;

  const addAction = () => {
    if (!canAdd || !schedule) return;
    const recurrence: CreateActionInput['recurrence'] = repeat === 'none' ? undefined : {
      frequency: repeat,
      interval: 1,
      weekdays: repeat === 'weekly' ? [weekdayForSchedule(schedule.scheduled_for)] : [],
    };
    onAddAction(journey.id, {
      title: title.trim(),
      detail: detail.trim(),
      estimated_minutes: Math.min(720, Math.max(1, Number(minutes))),
      stage_id: stageId,
      ...schedule,
      recurrence,
    });
    setAdding(false);
    setTitle('');
    setDetail('');
    setScheduleDay('today');
    setScheduleTime(defaultScheduleTime());
    setStageId(undefined);
    setRepeat('none');
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
          <Text style={styles.planMeta}>{journeyTypeLabels[journey.journey_type]} · {journey.completion_criteria || '按自己的节奏确认完成条件'}</Text>

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
                onPress={() => onPublish(journey, actions)}
                style={({ pressed }) => [styles.smallAction, pressed && styles.pressed]}
              >
                <Share2 color={colors.blue} size={17} />
                <Text style={styles.smallActionText}>发布路线</Text>
              </Pressable>
            </View>
          ) : null}

          {journey.stages.length ? (
            <View style={styles.stagesCard}>
              <Text style={styles.stagesTitle}>路线阶段</Text>
              {journey.stages.map((stage) => {
                const stageActions = actions.filter((action) => action.stage_id === stage.id);
                const completedActions = stageActions.filter((action) => action.state === 'completed').length;
                return <View key={stage.id} style={styles.stageRow}><View style={styles.stagePosition}><Text style={styles.stagePositionText}>{stage.position + 1}</Text></View><View style={styles.stageCopy}><Text style={styles.stageName}>{stage.title}</Text><Text style={styles.stageMeta}>{stage.completion_criteria || `${completedActions}/${stageActions.length} 项行动已完成`}</Text></View></View>;
              })}
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
              {journey.stages.length ? <View style={styles.stagePicker}><Text style={styles.stagePickerLabel}>所属阶段</Text><ScrollView contentContainerStyle={styles.stageOptions} horizontal showsHorizontalScrollIndicator={false}><Pressable accessibilityRole="radio" accessibilityState={{ checked: !stageId }} onPress={() => setStageId(undefined)} style={[styles.stageOption, !stageId && styles.stageOptionSelected]}><Text style={[styles.stageOptionText, !stageId && styles.stageOptionTextSelected]}>暂不归类</Text></Pressable>{journey.stages.map((stage) => <Pressable accessibilityRole="radio" accessibilityState={{ checked: stageId === stage.id }} key={stage.id} onPress={() => setStageId(stage.id)} style={[styles.stageOption, stageId === stage.id && styles.stageOptionSelected]}><Text style={[styles.stageOptionText, stageId === stage.id && styles.stageOptionTextSelected]}>{stage.title}</Text></Pressable>)}</ScrollView></View> : null}
              <View style={styles.scheduleRow}>
                <View style={styles.scheduleDays}>
                  {([['today', '今天'], ['tomorrow', '明天']] as const).map(([day, label]) => {
                    const selected = scheduleDay === day;
                    return <Pressable accessibilityRole="radio" accessibilityState={{ checked: selected }} key={day} onPress={() => setScheduleDay(day)} style={[styles.scheduleDay, selected && styles.scheduleDaySelected]}><Text style={[styles.scheduleDayText, selected && styles.scheduleDayTextSelected]}>{label}</Text></Pressable>;
                  })}
                </View>
                <TextInput accessibilityLabel="行动开始时间" autoCapitalize="none" maxLength={5} onChangeText={setScheduleTime} placeholder="19:00" placeholderTextColor={colors.faint} style={styles.scheduleTime} value={scheduleTime} />
              </View>
              {!schedule ? <Text style={styles.scheduleError}>请输入 24 小时制时间，例如 19:00。</Text> : null}
              <View style={styles.repeatRow}>
                {([['none', '不重复'], ['daily', '每天'], ['weekly', '每周']] as const).map(([value, label]) => <Pressable accessibilityRole="radio" accessibilityState={{ checked: repeat === value }} key={value} onPress={() => setRepeat(value)} style={[styles.repeatOption, repeat === value && styles.repeatOptionSelected]}><Text style={[styles.repeatOptionText, repeat === value && styles.repeatOptionTextSelected]}>{label}</Text></Pressable>)}
              </View>
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

function weekdayForSchedule(timestamp: string): Weekday {
  return weekdayByIndex[new Date(timestamp).getDay()] ?? 'monday';
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
  planMeta: { color: colors.faint, fontSize: 11, lineHeight: 17, marginTop: 8, letterSpacing: 0 },
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
  stagesCard: { marginTop: 12, padding: 15, gap: 11, borderRadius: 8, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  stagesTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  stageRow: { flexDirection: 'row', alignItems: 'flex-start', gap: 10 },
  stagePosition: { width: 20, height: 20, alignItems: 'center', justifyContent: 'center', borderRadius: 10, backgroundColor: colors.evergreenSoft },
  stagePositionText: { color: colors.evergreen, fontSize: 10, fontWeight: '800', letterSpacing: 0 },
  stageCopy: { flex: 1, minWidth: 0 },
  stageName: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  stageMeta: { color: colors.faint, fontSize: 11, lineHeight: 16, marginTop: 2, letterSpacing: 0 },
  sectionHeader: { height: 64, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 16 },
  sectionTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  addButton: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center', borderRadius: 6, backgroundColor: colors.evergreenSoft },
  addForm: { padding: 13, gap: 9, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line, marginBottom: 10 },
  input: { minHeight: 42, borderRadius: 5, borderWidth: 1, borderColor: colors.line, paddingHorizontal: 10, color: colors.ink, backgroundColor: colors.background, fontSize: 13, letterSpacing: 0 },
  stagePicker: { gap: 6 },
  stagePickerLabel: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  stageOptions: { gap: 7 },
  stageOption: { height: 30, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderRadius: 15, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background },
  stageOptionSelected: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  stageOptionText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  stageOptionTextSelected: { color: colors.evergreen },
  scheduleRow: { height: 40, flexDirection: 'row', gap: 8 },
  scheduleDays: { flex: 1, flexDirection: 'row', overflow: 'hidden', borderWidth: 1, borderColor: colors.line, borderRadius: 5 },
  scheduleDay: { flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.background },
  scheduleDaySelected: { backgroundColor: colors.evergreen },
  scheduleDayText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  scheduleDayTextSelected: { color: colors.surface },
  scheduleTime: { width: 82, borderRadius: 5, borderWidth: 1, borderColor: colors.line, paddingHorizontal: 10, color: colors.ink, backgroundColor: colors.background, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  scheduleError: { marginTop: -3, color: colors.coral, fontSize: 11, lineHeight: 16, letterSpacing: 0 },
  repeatRow: { height: 34, flexDirection: 'row', overflow: 'hidden', borderWidth: 1, borderColor: colors.line, borderRadius: 5 },
  repeatOption: { flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.background },
  repeatOptionSelected: { backgroundColor: colors.evergreen },
  repeatOptionText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  repeatOptionTextSelected: { color: colors.surface },
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
