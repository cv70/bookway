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
import { CreateJourneyInput, GrowthDomain } from '../types';

const domains = Object.keys(domainMeta) as GrowthDomain[];

type Props = {
  visible: boolean;
  onClose: () => void;
  onSubmit: (input: CreateJourneyInput) => void;
};

export function CreateJourneyModal({ visible, onClose, onSubmit }: Props) {
  const [title, setTitle] = useState('');
  const [intent, setIntent] = useState('');
  const [domain, setDomain] = useState<GrowthDomain>('learning');
  const [firstAction, setFirstAction] = useState('');
  const [detail, setDetail] = useState('');
  const ready = title.trim().length > 0 && firstAction.trim().length > 0;

  const submit = () => {
    if (!ready) return;
    onSubmit({
      title: title.trim(),
      intent: intent.trim(),
      domain,
      duration_label: '4 周',
      first_action_title: firstAction.trim(),
      first_action_detail: detail.trim(),
      estimated_minutes: 20,
    });
    setTitle('');
    setIntent('');
    setFirstAction('');
    setDetail('');
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
            <Field label="第一个行动" placeholder="从一件今天能完成的小事开始" value={firstAction} onChange={setFirstAction} />
            <Field label="行动备注" placeholder="地点、标准或提醒" value={detail} onChange={setDetail} />
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

type FieldProps = {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  multiline?: boolean;
};

function Field({ label, placeholder, value, onChange, multiline }: FieldProps) {
  return (
    <View style={styles.field}>
      <Text style={styles.label}>{label}</Text>
      <TextInput
        multiline={multiline}
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
  segment: { flex: 1, height: 40, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.surface },
  segmentSelected: { backgroundColor: colors.evergreen },
  segmentText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  segmentTextSelected: { color: colors.surface },
  submit: { height: 50, marginHorizontal: 20, marginTop: 4, borderRadius: 6, backgroundColor: colors.evergreen, alignItems: 'center', justifyContent: 'center' },
  submitDisabled: { opacity: 0.35 },
  pressed: { opacity: 0.72 },
  submitText: { color: colors.surface, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
});

