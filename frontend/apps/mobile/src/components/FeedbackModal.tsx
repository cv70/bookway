import { MessageSquarePlus, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import { getMyFeedback, submitFeedback } from '../api/client';
import { colors } from '../theme';
import { FeedbackCategory, UserFeedback } from '../types';

type Props = {
  visible: boolean;
  onClose: () => void;
};

const categories: Array<{ value: FeedbackCategory; label: string }> = [
  { value: 'bug', label: '功能异常' },
  { value: 'feature', label: '功能建议' },
  { value: 'experience', label: '使用体验' },
  { value: 'content', label: '内容问题' },
  { value: 'other', label: '其他' },
];

export function FeedbackModal({ visible, onClose }: Props) {
  const [category, setCategory] = useState<FeedbackCategory>('experience');
  const [content, setContent] = useState('');
  const [contact, setContact] = useState('');
  const [history, setHistory] = useState<UserFeedback[]>([]);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();

  useEffect(() => {
    if (!visible) return undefined;
    let active = true;
    setLoading(true);
    setError(undefined);
    void getMyFeedback()
      .then((items) => {
        if (active) setHistory(items);
      })
      .catch(() => {
        if (active) setError('暂时无法读取反馈记录。');
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [visible]);

  const submit = async () => {
    if (!content.trim() || submitting) return;
    setSubmitting(true);
    setError(undefined);
    setNotice(undefined);
    try {
      const feedback = await submitFeedback({ category, content, contact });
      setHistory((current) => [feedback, ...current.filter((item) => item.id !== feedback.id)]);
      setContent('');
      setContact('');
      setNotice('已收到你的反馈，感谢帮助我们把 Bookway 做得更好。');
    } catch {
      setError('反馈未能提交，草稿已保留，请检查网络后重试。');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} presentationStyle="pageSheet" visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭意见反馈" onPress={onClose} style={styles.close}><X color={colors.ink} size={21} /></Pressable>
          <Text style={styles.title}>意见反馈</Text>
          <View style={styles.close} />
        </View>
        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
          <View style={styles.hero}>
            <View style={styles.heroIcon}><MessageSquarePlus color={colors.evergreen} size={23} /></View>
            <View style={styles.heroCopy}>
              <Text style={styles.heroTitle}>告诉我们你的真实感受</Text>
              <Text style={styles.heroText}>问题、建议和不顺手的细节都会被认真记录与处理。</Text>
            </View>
          </View>
          <Text style={styles.label}>反馈类型</Text>
          <View style={styles.categories}>
            {categories.map((item) => (
              <Pressable accessibilityRole="button" accessibilityState={{ selected: category === item.value }} key={item.value} onPress={() => setCategory(item.value)} style={[styles.category, category === item.value && styles.categorySelected]}>
                <Text style={[styles.categoryText, category === item.value && styles.categoryTextSelected]}>{item.label}</Text>
              </Pressable>
            ))}
          </View>
          <View style={styles.labelRow}><Text style={styles.label}>具体内容</Text><Text style={styles.count}>{content.length}/2000</Text></View>
          <TextInput accessibilityLabel="反馈内容" maxLength={2000} multiline onChangeText={(value) => { setContent(value); setError(undefined); setNotice(undefined); }} placeholder="描述你遇到的问题，或想看到的改变" placeholderTextColor={colors.faint} style={styles.contentInput} textAlignVertical="top" value={content} />
          <Text style={styles.hint}>请不要填写密码、身份证或支付信息等敏感内容。</Text>
          <Text style={styles.label}>联系方式 <Text style={styles.optional}>（选填）</Text></Text>
          <TextInput accessibilityLabel="反馈联系方式（选填）" maxLength={200} onChangeText={(value) => { setContact(value); setError(undefined); }} placeholder="邮箱或其他方便回复的方式" placeholderTextColor={colors.faint} style={styles.contactInput} value={contact} />
          {error ? <Text accessibilityLiveRegion="polite" style={styles.error}>{error}</Text> : null}
          {notice ? <Text accessibilityLiveRegion="polite" style={styles.notice}>{notice}</Text> : null}
          <Pressable accessibilityLabel="提交意见反馈" disabled={!content.trim() || submitting} onPress={() => void submit()} style={[styles.submit, (!content.trim() || submitting) && styles.submitDisabled]}>
            <Text style={styles.submitText}>{submitting ? '正在提交…' : '提交反馈'}</Text>
          </Pressable>
          <Text style={styles.historyTitle}>我的反馈</Text>
          {loading ? <Text accessibilityLiveRegion="polite" style={styles.historyState}>正在读取反馈记录…</Text> : null}
          {!loading && !history.length ? <Text style={styles.historyState}>还没有提交过反馈。</Text> : null}
          {history.map((item) => <FeedbackHistoryCard feedback={item} key={item.id} />)}
        </ScrollView>
      </View>
    </Modal>
  );
}

function FeedbackHistoryCard({ feedback }: { feedback: UserFeedback }) {
  return <View style={styles.historyCard}>
    <View style={styles.historyHeader}><Text style={styles.historyCategory}>{categoryLabel(feedback.category)}</Text><Text style={styles.historyStatus}>{statusLabel(feedback.status)}</Text></View>
    <Text numberOfLines={3} style={styles.historyContent}>{feedback.content}</Text>
    {feedback.resolution ? <Text style={styles.resolution}>处理说明：{feedback.resolution}</Text> : null}
    <Text style={styles.date}>{formatDate(feedback.updated_at || feedback.created_at)}</Text>
  </View>;
}

function categoryLabel(category: FeedbackCategory) {
  return categories.find((item) => item.value === category)?.label ?? '其他';
}

function statusLabel(status: UserFeedback['status']) {
  if (status === 'pending') return '待受理';
  if (status === 'processing') return '处理中';
  return status === 'resolved' ? '已处理' : '已关闭';
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '时间待确认' : `${date.getFullYear()} 年 ${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, textAlign: 'center', color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  content: { padding: 20, paddingBottom: 42 },
  hero: { padding: 15, flexDirection: 'row', gap: 12, borderRadius: 8, backgroundColor: colors.evergreenSoft },
  heroIcon: { width: 42, height: 42, borderRadius: 7, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.surface },
  heroCopy: { flex: 1, minWidth: 0 },
  heroTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  heroText: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 4, letterSpacing: 0 },
  label: { color: colors.ink, fontSize: 13, fontWeight: '700', marginTop: 22, letterSpacing: 0 },
  labelRow: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'baseline' },
  count: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  optional: { color: colors.faint, fontWeight: '400' },
  categories: { marginTop: 10, flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  category: { minHeight: 34, paddingHorizontal: 12, justifyContent: 'center', borderRadius: 5, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  categorySelected: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  categoryText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  categoryTextSelected: { color: colors.evergreen },
  contentInput: { minHeight: 128, marginTop: 10, padding: 12, color: colors.ink, fontSize: 13, lineHeight: 21, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface, letterSpacing: 0 },
  hint: { color: colors.faint, fontSize: 10, lineHeight: 16, marginTop: 6, letterSpacing: 0 },
  contactInput: { height: 46, marginTop: 10, paddingHorizontal: 12, color: colors.ink, fontSize: 13, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface, letterSpacing: 0 },
  error: { color: colors.coral, fontSize: 12, lineHeight: 18, marginTop: 10, letterSpacing: 0 },
  notice: { color: colors.evergreen, fontSize: 12, lineHeight: 18, marginTop: 10, letterSpacing: 0 },
  submit: { minHeight: 46, marginTop: 17, alignItems: 'center', justifyContent: 'center', borderRadius: 6, backgroundColor: colors.evergreen },
  submitDisabled: { opacity: 0.45 },
  submitText: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  historyTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', marginTop: 31, marginBottom: 5, letterSpacing: 0 },
  historyState: { paddingVertical: 17, color: colors.faint, fontSize: 12, textAlign: 'center', letterSpacing: 0 },
  historyCard: { paddingVertical: 13, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  historyHeader: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  historyCategory: { color: colors.ink, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  historyStatus: { color: colors.evergreen, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  historyContent: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 6, letterSpacing: 0 },
  resolution: { color: colors.ink, fontSize: 12, lineHeight: 19, marginTop: 7, letterSpacing: 0 },
  date: { color: colors.faint, fontSize: 10, marginTop: 8, letterSpacing: 0 },
});
