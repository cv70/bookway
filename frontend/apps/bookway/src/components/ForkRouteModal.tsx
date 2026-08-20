import { GitFork, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { KeyboardAvoidingView, Modal, Platform, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';

import { colors } from '../theme';

type Props = {
  sourceTitle: string;
  visible: boolean;
  onClose: () => void;
  onSubmit: (title: string, summary: string) => Promise<void>;
};

export function ForkRouteModal({ sourceTitle, visible, onClose, onSubmit }: Props) {
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState(false);

  useEffect(() => {
    if (!visible) return;
    setTitle(`${sourceTitle.trim() || '这条路线'}（分支）`);
    setSummary('');
    setSubmitting(false);
    setError(false);
  }, [sourceTitle, visible]);

  const submit = async () => {
    if (!title.trim() || submitting) return;
    setSubmitting(true);
    setError(false);
    try {
      await onSubmit(title.trim(), summary.trim());
    } catch {
      setError(true);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} transparent visible={visible}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.overlay}>
        <View style={styles.sheet}>
          <View style={styles.header}>
            <View style={styles.heading}><GitFork color={colors.evergreen} size={20} /><Text style={styles.title}>分叉这条路线</Text></View>
            <Pressable accessibilityLabel="关闭分叉路线" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={21} /></Pressable>
          </View>
          <View style={styles.form}>
            <Text style={styles.source}>来源：{sourceTitle || '公开路线'}</Text>
            <Field label="你的路线名称" value={title} onChange={setTitle} placeholder="给新的路线一个名字" />
            <Field label="路线摘要（可选）" value={summary} onChange={setSummary} placeholder="保留原意，也可以写下你的调整方向" multiline />
            <Text style={styles.note}>系统会复制公开的行动节点快照，创建一份只属于你的可编辑草稿。原作者的私密记录和媒体不会被复制。</Text>
            {error ? <Text accessibilityLiveRegion="polite" style={styles.error}>创建失败，请检查网络后重试</Text> : null}
          </View>
          <Pressable accessibilityRole="button" disabled={!title.trim() || submitting} onPress={() => void submit()} style={({ pressed }) => [styles.submit, (!title.trim() || submitting) && styles.disabled, pressed && styles.pressed]}>
            <Text style={styles.submitText}>{submitting ? '正在创建…' : '创建可编辑草稿'}</Text>
          </Pressable>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

function Field({ label, value, onChange, placeholder, multiline = false }: { label: string; value: string; onChange: (value: string) => void; placeholder: string; multiline?: boolean }) {
  return <View style={styles.field}><Text style={styles.label}>{label}</Text><TextInput maxLength={multiline ? 300 : 120} multiline={multiline} onChangeText={onChange} placeholder={placeholder} placeholderTextColor={colors.faint} style={[styles.input, multiline && styles.multiline]} textAlignVertical={multiline ? 'top' : 'center'} value={value} /></View>;
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(24, 29, 26, 0.36)' },
  sheet: { maxHeight: '82%', paddingBottom: 28, backgroundColor: colors.surface, borderTopLeftRadius: 8, borderTopRightRadius: 8 },
  header: { minHeight: 64, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  heading: { flexDirection: 'row', alignItems: 'center', gap: 9 },
  title: { color: colors.ink, fontSize: 18, fontWeight: '700', letterSpacing: 0 },
  close: { width: 38, height: 38, alignItems: 'center', justifyContent: 'center' },
  form: { padding: 20, gap: 16 },
  source: { color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  field: { gap: 7 },
  label: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  input: { minHeight: 46, paddingHorizontal: 12, paddingVertical: 10, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  multiline: { minHeight: 80 },
  note: { color: colors.faint, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  error: { color: colors.coral, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  submit: { height: 48, marginHorizontal: 20, alignItems: 'center', justifyContent: 'center', borderRadius: 6, backgroundColor: colors.evergreen },
  submitText: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.35 },
  pressed: { opacity: 0.68 },
});
