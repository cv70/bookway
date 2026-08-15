import { X } from 'lucide-react-native';
import { useState } from 'react';
import {
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { colors, domainMeta } from '../theme';
import { CreateJourneyInput, GrowthDomain, JourneyType, Weekday } from '../types';
import { defaultScheduleTime, scheduleForDay, ScheduleDay } from '../utils/scheduling';

const domains = Object.keys(domainMeta) as GrowthDomain[];
const journeyTypes: Array<{ value: JourneyType; label: string }> = [
  { value: 'habit', label: '习惯' },
  { value: 'project', label: '项目' },
  { value: 'quantity', label: '数量' },
  { value: 'travel', label: '旅程' },
  { value: 'challenge', label: '挑战' },
];
const weekdays: Weekday[] = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];

type Props = {
  visible: boolean;
  onClose: () => void;
  onSubmit: (input: CreateJourneyInput) => void;
};

export function CreateJourneyModal({ visible, onClose, onSubmit }: Props) {
  const [title, setTitle] = useState('');
  const [intent, setIntent] = useState('');
  const [domain, setDomain] = useState<GrowthDomain>('learning');
  const [journeyType, setJourneyType] = useState<JourneyType>('project');
  const [completionCriteria, setCompletionCriteria] = useState('');
  const [firstStage, setFirstStage] = useState('');
  const [stageCriteria, setStageCriteria] = useState('');
  const [duration, setDuration] = useState('4 周');
  const [firstAction, setFirstAction] = useState('');
  const [detail, setDetail] = useState('');
  const [minutes, setMinutes] = useState('20');
  const [scheduleDay, setScheduleDay] = useState<ScheduleDay>('today');
  const [scheduleTime, setScheduleTime] = useState(defaultScheduleTime());
  const [repeat, setRepeat] = useState<'none' | 'daily' | 'weekly'>('none');
  const schedule = scheduleForDay(scheduleDay, scheduleTime);
  const ready = title.trim().length > 0 && firstAction.trim().length > 0 && Number(minutes) > 0 && schedule !== null;

  const submit = () => {
    if (!ready || !schedule) return;
    const stages = firstStage.trim()
      ? [{ title: firstStage.trim(), detail: '', completion_criteria: stageCriteria.trim() }]
      : [];
    const recurrence: CreateJourneyInput['first_action_recurrence'] = repeat === 'none' ? undefined : {
      frequency: repeat,
      interval: 1,
      weekdays: repeat === 'weekly' ? [weekdayForSchedule(schedule.scheduled_for)] : [],
    };
    onSubmit({
      title: title.trim(),
      intent: intent.trim(),
      domain,
      journey_type: journeyType,
      completion_criteria: completionCriteria.trim(),
      stages,
      duration_label: duration,
      first_action_title: firstAction.trim(),
      first_action_detail: detail.trim(),
      estimated_minutes: Math.min(720, Math.max(1, Number(minutes))),
      first_action_scheduled_label: schedule.scheduled_label,
      first_action_scheduled_for: schedule.scheduled_for,
      first_action_scheduled_timezone: schedule.scheduled_timezone,
      first_action_stage_index: stages.length ? 0 : undefined,
      first_action_recurrence: recurrence,
    });
    setTitle('');
    setIntent('');
    setJourneyType('project');
    setCompletionCriteria('');
    setFirstStage('');
    setStageCriteria('');
    setFirstAction('');
    setDetail('');
    setDuration('4 周');
    setMinutes('20');
    setScheduleDay('today');
    setScheduleTime(defaultScheduleTime());
    setRepeat('none');
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} transparent visible={visible}>
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        style={styles.overlay}
      >
        <View style={styles.sheet}>
          <View style={styles.header}>
            <Text style={styles.title}>创建路线</Text>
            <Pressable accessibilityLabel="关闭" hitSlop={10} onPress={onClose} style={styles.close}>
              <X color={colors.ink} size={22} />
            </Pressable>
          </View>
          <ScrollView contentContainerStyle={styles.form} keyboardShouldPersistTaps="handled">
            <Field label="路线名称" placeholder="例如：重新开始跑步" value={title} onChange={setTitle} />
            <Field
              label="为什么出发"
              placeholder="写下你希望发生的改变"
              value={intent}
              onChange={setIntent}
              multiline
            />
            <View style={styles.field}>
              <Text style={styles.label}>方向</Text>
              <View style={styles.segmented}>
                {domains.map((item) => {
                  const selected = item === domain;
                  return (
                    <Pressable
                      accessibilityRole="radio"
                      accessibilityState={{ checked: selected }}
                      key={item}
                      onPress={() => setDomain(item)}
                      style={[styles.segment, selected && styles.segmentSelected]}
                    >
                      <Text style={[styles.segmentText, selected && styles.segmentTextSelected]}>
                        {domainMeta[item].label}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>
            <View style={styles.field}>
              <Text style={styles.label}>路线类型</Text>
              <View style={styles.typeGrid}>
                {journeyTypes.map((item) => {
                  const selected = item.value === journeyType;
                  return (
                    <Pressable
                      accessibilityRole="radio"
                      accessibilityState={{ checked: selected }}
                      key={item.value}
                      onPress={() => setJourneyType(item.value)}
                      style={[styles.typeOption, selected && styles.typeOptionSelected]}
                    >
                      <Text style={[styles.typeOptionText, selected && styles.typeOptionTextSelected]}>{item.label}</Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>
            <Field
              label="完成标准（可选）"
              placeholder="例如：四周内完成 12 次练习"
              value={completionCriteria}
              onChange={setCompletionCriteria}
            />
            <View style={styles.field}>
              <Text style={styles.label}>预计周期</Text>
              <View style={styles.segmented}>
                {['2 周', '4 周', '6 周', '长期'].map((item) => {
                  const selected = item === duration;
                  return <Pressable accessibilityRole="radio" accessibilityState={{ checked: selected }} key={item} onPress={() => setDuration(item)} style={[styles.segment, selected && styles.segmentSelected]}><Text style={[styles.segmentText, selected && styles.segmentTextSelected]}>{item}</Text></Pressable>;
                })}
              </View>
            </View>
            <Field label="第一个行动" placeholder="从一件今天能完成的小事开始" value={firstAction} onChange={setFirstAction} />
            <Field label="行动备注" placeholder="地点、标准或提醒" value={detail} onChange={setDetail} />
            <Field label="第一阶段（可选）" placeholder="例如：恢复节奏" value={firstStage} onChange={setFirstStage} />
            {firstStage.trim() ? <Field label="阶段完成标准（可选）" placeholder="例如：完成三次轻松跑" value={stageCriteria} onChange={setStageCriteria} /> : null}
            <Field label="预计用时（分钟）" placeholder="20" value={minutes} onChange={setMinutes} keyboardType="number-pad" />
            <View style={styles.field}>
              <Text style={styles.label}>安排第一个行动</Text>
              <View style={styles.segmented}>
                {([['today', '今天'], ['tomorrow', '明天']] as const).map(([day, label]) => {
                  const selected = scheduleDay === day;
                  return <Pressable accessibilityRole="radio" accessibilityState={{ checked: selected }} key={day} onPress={() => setScheduleDay(day)} style={[styles.segment, selected && styles.segmentSelected]}><Text style={[styles.segmentText, selected && styles.segmentTextSelected]}>{label}</Text></Pressable>;
                })}
              </View>
              <TextInput
                accessibilityLabel="第一个行动的开始时间"
                autoCapitalize="none"
                maxLength={5}
                onChangeText={setScheduleTime}
                placeholder="19:00"
                placeholderTextColor={colors.faint}
                style={styles.input}
                value={scheduleTime}
              />
              {schedule ? <Text style={styles.scheduleHint}>将在 {schedule.scheduled_label} 开始，可随时在行动详情改期。</Text> : <Text style={styles.scheduleError}>请输入 24 小时制时间，例如 19:00。</Text>}
            </View>
            <View style={styles.field}>
              <Text style={styles.label}>重复这个行动</Text>
              <View style={styles.segmented}>
                {([['none', '不重复'], ['daily', '每天'], ['weekly', '每周同一天']] as const).map(([value, label]) => {
                  const selected = repeat === value;
                  return <Pressable accessibilityRole="radio" accessibilityState={{ checked: selected }} key={value} onPress={() => setRepeat(value)} style={[styles.segment, selected && styles.segmentSelected]}><Text style={[styles.segmentText, selected && styles.segmentTextSelected]}>{label}</Text></Pressable>;
                })}
              </View>
              {repeat === 'weekly' && schedule ? <Text style={styles.scheduleHint}>每周会在首个安排对应的星期重复一次。</Text> : null}
            </View>
          </ScrollView>
          <Pressable
            accessibilityRole="button"
            disabled={!ready}
            onPress={submit}
            style={({ pressed }) => [styles.submit, !ready && styles.submitDisabled, pressed && ready && styles.pressed]}
          >
            <Text style={styles.submitText}>开始这段路</Text>
          </Pressable>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

function weekdayForSchedule(timestamp: string): Weekday {
  return weekdays[new Date(timestamp).getDay()] ?? 'monday';
}

type FieldProps = {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  multiline?: boolean;
  keyboardType?: 'default' | 'number-pad';
};

function Field({ label, placeholder, value, onChange, multiline, keyboardType }: FieldProps) {
  return (
    <View style={styles.field}>
      <Text style={styles.label}>{label}</Text>
      <TextInput
        multiline={multiline}
        keyboardType={keyboardType}
        onChangeText={onChange}
        placeholder={placeholder}
        placeholderTextColor={colors.faint}
        style={[styles.input, multiline && styles.multiline]}
        value={value}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(24, 29, 26, 0.36)' },
  sheet: { maxHeight: '92%', backgroundColor: colors.surface, borderTopLeftRadius: 8, borderTopRightRadius: 8, paddingBottom: 28 },
  header: { height: 66, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  title: { color: colors.ink, fontSize: 20, lineHeight: 28, fontWeight: '700', letterSpacing: 0 },
  close: { width: 38, height: 38, alignItems: 'center', justifyContent: 'center' },
  form: { padding: 20, gap: 18 },
  field: { gap: 7 },
  label: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  input: { minHeight: 46, borderWidth: 1, borderColor: colors.line, borderRadius: 6, paddingHorizontal: 12, paddingVertical: 10, color: colors.ink, backgroundColor: colors.background, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  multiline: { minHeight: 76, textAlignVertical: 'top' },
  segmented: { flexDirection: 'row', borderWidth: 1, borderColor: colors.line, borderRadius: 6, overflow: 'hidden' },
  typeGrid: { flexDirection: 'row', flexWrap: 'wrap', gap: 7 },
  typeOption: { minWidth: 58, height: 34, paddingHorizontal: 11, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: colors.line, borderRadius: 17, backgroundColor: colors.surface },
  typeOptionSelected: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  typeOptionText: { color: colors.muted, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  typeOptionTextSelected: { color: colors.evergreen },
  segment: { flex: 1, height: 40, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.surface },
  segmentSelected: { backgroundColor: colors.evergreen },
  segmentText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  segmentTextSelected: { color: colors.surface },
  scheduleHint: { color: colors.muted, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  scheduleError: { color: colors.coral, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  submit: { height: 50, marginHorizontal: 20, marginTop: 4, borderRadius: 6, backgroundColor: colors.evergreen, alignItems: 'center', justifyContent: 'center' },
  submitDisabled: { opacity: 0.35 },
  pressed: { opacity: 0.72 },
  submitText: { color: colors.surface, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
});
