import { Check, Clock3, GitFork, Save, X } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import { KeyboardAvoidingView, Modal, Platform, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import { colors } from '../theme';
import { ContentDetail, OwnedContent, RouteTemplate, RouteTemplateAction, UpdatePostInput } from '../types';

type DraftContent = ContentDetail | OwnedContent;

type Props = {
  content?: DraftContent;
  visible: boolean;
  onClose: () => void;
  onSave: (contentId: string, input: UpdatePostInput) => Promise<ContentDetail>;
  onPublish: (contentId: string, input: UpdatePostInput) => Promise<void>;
};

export function RouteDraftModal({ content, visible, onClose, onSave, onPublish }: Props) {
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [body, setBody] = useState('');
  const [intent, setIntent] = useState('');
  const [completionCriteria, setCompletionCriteria] = useState('');
  const [stages, setStages] = useState<RouteTemplate['stages']>([]);
  const [actions, setActions] = useState<RouteTemplateAction[]>([]);
  const [saving, setSaving] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  const hydrate = (next: DraftContent) => {
    setTitle(next.post?.title ?? '');
    setSummary(next.post?.summary ?? '');
    setBody(next.body ?? '');
    setIntent(next.route_template?.intent ?? '');
    setCompletionCriteria(next.route_template?.completion_criteria ?? '');
    setStages(next.route_template?.stages.map((stage) => ({ ...stage })) ?? []);
    setActions(next.route_template?.actions.map((action) => ({ ...action, scene_equipment: [...(action.scene_equipment ?? [])] })) ?? []);
    setError('');
    setNotice('');
  };

  useEffect(() => {
    if (!visible || !content) return;
    hydrate(content);
  }, [content, visible]);

  const routeTemplate = useMemo<RouteTemplate | undefined>(() => {
    if (!content?.route_template) return undefined;
    return {
      intent: intent.trim(),
      completion_criteria: completionCriteria.trim(),
      stages: stages.map((stage) => ({ ...stage })),
      actions: actions.map((action) => ({
        ...action,
        title: action.title.trim(),
        detail: action.detail.trim(),
        scheduled_label: action.scheduled_label.trim(),
        estimated_minutes: Math.min(720, Math.max(1, Math.floor(action.estimated_minutes))),
        scene_equipment: normalizeEquipment(action.scene_equipment ?? []),
      })),
      journey_type: content.route_template.journey_type,
    };
  }, [actions, completionCriteria, content, intent, stages]);

  const input = useMemo<UpdatePostInput | undefined>(() => {
    if (!routeTemplate) return undefined;
    return {
      title: title.trim(),
      summary: summary.trim(),
      body: body.trim(),
      route_template: routeTemplate,
    };
  }, [body, routeTemplate, summary, title]);

  const ready = Boolean(
    content
    && input
    && input.title
    && input.body
    && input.route_template?.intent
    && input.route_template.completion_criteria
    && input.route_template.actions.length > 0
    && input.route_template.actions.every((action) => action.title && action.scheduled_label && action.estimated_minutes >= 1 && action.estimated_minutes <= 720),
  );

  const save = async () => {
    if (!content || !input || !ready || saving || publishing) return;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      const updated = await onSave(content.id, input);
      hydrate(updated);
      setNotice('草稿已保存');
    } catch {
      setError('保存失败，修改仍保留在当前页面');
    } finally {
      setSaving(false);
    }
  };

  const publish = async () => {
    if (!content || !input || !ready || saving || publishing) return;
    setPublishing(true);
    setError('');
    setNotice('');
    try {
      await onPublish(content.id, input);
    } catch {
      setError('提交审核失败，草稿仍然保留');
    } finally {
      setPublishing(false);
    }
  };

  if (!content || !content.route_template) return null;
  const fork = content.route_fork;

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.screen}>
        <View style={styles.header}>
          <View style={styles.heading}><GitFork color={colors.evergreen} size={20} /><Text style={styles.headerTitle}>编辑路线草稿</Text></View>
          <Pressable accessibilityLabel="关闭路线草稿" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable>
        </View>
        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
          {fork ? <View style={styles.provenance}><Text style={styles.provenanceTitle}>来自公开路线：{fork.source_route_title}</Text><Text style={styles.provenanceMeta}>已固定来源版本 {fork.source_route_version} · 原作者的私密内容不会被带入</Text></View> : null}
          <Field label="路线名称" value={title} onChange={setTitle} placeholder="路线名称" />
          <Field label="路线摘要" multiline value={summary} onChange={setSummary} placeholder="说明这条路线适合谁" />
          <Field label="路线正文" multiline value={body} onChange={setBody} placeholder="写下你会如何走这条路线" />
          <Field label="路线意图" value={intent} onChange={setIntent} placeholder="希望达成什么改变" />
          <Field label="完成标准" value={completionCriteria} onChange={setCompletionCriteria} placeholder="怎样算完成" />
          <Text style={styles.sectionTitle}>行动节点</Text>
          <Text style={styles.sectionHint}>节点 ID 由系统维护；你可以调整行动内容和场景装备，已有商业挂载会按稳定 ID 重新校验。</Text>
          {actions.map((action, index) => <ActionEditor action={action} index={index} key={action.id} onChange={(updates) => setActions((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, ...updates } : item))} />)}
          {notice ? <Text accessibilityLiveRegion="polite" style={styles.notice}>{notice}</Text> : null}
          {error ? <Text accessibilityLiveRegion="polite" style={styles.error}>{error}</Text> : null}
        </ScrollView>
        <View style={styles.footer}>
          <Pressable accessibilityRole="button" disabled={!ready || saving || publishing} onPress={() => void save()} style={({ pressed }) => [styles.secondaryButton, (!ready || saving || publishing) && styles.disabled, pressed && styles.pressed]}><Save color={colors.evergreen} size={17} /><Text style={styles.secondaryText}>{saving ? '保存中…' : '保存草稿'}</Text></Pressable>
          <Pressable accessibilityRole="button" disabled={!ready || saving || publishing} onPress={() => void publish()} style={({ pressed }) => [styles.primaryButton, (!ready || saving || publishing) && styles.disabled, pressed && styles.pressed]}><Check color={colors.surface} size={17} /><Text style={styles.primaryText}>{publishing ? '提交中…' : '提交审核'}</Text></Pressable>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

function ActionEditor({ action, index, onChange }: { action: RouteTemplateAction; index: number; onChange: (updates: Partial<RouteTemplateAction>) => void }) {
  const equipment = (action.scene_equipment ?? []).join('、');
  return <View style={styles.actionCard}><View style={styles.actionHeading}><Text style={styles.actionIndex}>节点 {index + 1}</Text><Text numberOfLines={1} style={styles.actionId}>{action.id}</Text></View><Field label="行动名称" value={action.title} onChange={(value) => onChange({ title: value })} placeholder="行动名称" /><Field label="行动说明" multiline value={action.detail} onChange={(value) => onChange({ detail: value })} placeholder="行动说明" /><View style={styles.row}><View style={styles.duration}><Clock3 color={colors.muted} size={15} /><TextInput accessibilityLabel={`节点 ${index + 1} 预计分钟`} keyboardType="number-pad" maxLength={3} onChangeText={(value) => onChange({ estimated_minutes: Number(value.replace(/[^0-9]/g, '')) || 0 })} placeholder="20" placeholderTextColor={colors.faint} style={styles.durationInput} value={String(action.estimated_minutes || '')} /><Text style={styles.unit}>分钟</Text></View></View><Field label="场景装备（用顿号分隔）" value={equipment} onChange={(value) => onChange({ scene_equipment: normalizeEquipment(value.split(/[,，、]/)) })} placeholder="例如：阅读灯、计时器" /></View>;
}

function normalizeEquipment(values: string[]) {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const raw of values) {
    const value = raw.trim();
    const key = value.toLowerCase();
    if (!value || seen.has(key)) continue;
    seen.add(key);
    normalized.push(value);
    if (normalized.length === 12) break;
  }
  return normalized;
}

function Field({ label, value, onChange, placeholder, multiline = false }: { label: string; value: string; onChange: (value: string) => void; placeholder: string; multiline?: boolean }) {
  return <View style={styles.field}><Text style={styles.label}>{label}</Text><TextInput maxLength={multiline ? 1000 : 300} multiline={multiline} onChangeText={onChange} placeholder={placeholder} placeholderTextColor={colors.faint} style={[styles.input, multiline && styles.multiline]} textAlignVertical={multiline ? 'top' : 'center'} value={value} /></View>;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { minHeight: 64, paddingHorizontal: 17, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line, backgroundColor: colors.surface },
  heading: { flexDirection: 'row', alignItems: 'center', gap: 9 },
  headerTitle: { color: colors.ink, fontSize: 17, fontWeight: '700', letterSpacing: 0 },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  content: { padding: 20, gap: 16 },
  provenance: { padding: 13, gap: 4, borderWidth: 1, borderColor: colors.evergreenSoft, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  provenanceTitle: { color: colors.evergreen, fontSize: 13, fontWeight: '700', lineHeight: 19, letterSpacing: 0 },
  provenanceMeta: { color: colors.muted, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  field: { gap: 7 },
  label: { color: colors.ink, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  input: { minHeight: 44, paddingHorizontal: 11, paddingVertical: 9, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.surface, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  multiline: { minHeight: 76 },
  sectionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  sectionHint: { marginTop: -9, color: colors.muted, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  actionCard: { padding: 13, gap: 12, borderWidth: 1, borderColor: colors.line, borderRadius: 7, backgroundColor: colors.surface },
  actionHeading: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  actionIndex: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  actionId: { flex: 1, color: colors.faint, fontSize: 10, letterSpacing: 0 },
  row: { flexDirection: 'row', alignItems: 'center' },
  duration: { height: 40, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', gap: 7, borderWidth: 1, borderColor: colors.line, borderRadius: 6, backgroundColor: colors.background },
  durationInput: { width: 54, paddingVertical: 4, color: colors.ink, fontSize: 13, letterSpacing: 0 },
  unit: { color: colors.muted, fontSize: 11, letterSpacing: 0 },
  notice: { color: colors.evergreen, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  error: { color: colors.coral, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  footer: { minHeight: 70, paddingHorizontal: 20, paddingVertical: 11, flexDirection: 'row', gap: 10, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line, backgroundColor: colors.surface },
  secondaryButton: { flex: 1, height: 46, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderWidth: 1, borderColor: colors.evergreen, borderRadius: 6 },
  secondaryText: { color: colors.evergreen, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  primaryButton: { flex: 1, height: 46, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderRadius: 6, backgroundColor: colors.evergreen },
  primaryText: { color: colors.surface, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.35 },
  pressed: { opacity: 0.68 },
});
