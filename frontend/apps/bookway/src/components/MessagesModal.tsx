import { ArrowLeft, Mail, RefreshCw, Send, X } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, Alert, Modal, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import { getDirectConversations, getDirectMessages, markDirectConversationRead, reportDirectMessage, sendDirectMessage } from '../api/client';
import { colors } from '../theme';
import { DirectConversation, DirectMessage } from '../types';

type Props = {
  visible: boolean;
  initialRecipientId?: string;
  onClose: () => void;
};

export function MessagesModal({ visible, initialRecipientId, onClose }: Props) {
  const [conversations, setConversations] = useState<DirectConversation[]>([]);
  const [selected, setSelected] = useState<DirectConversation>();
  const [messages, setMessages] = useState<DirectMessage[]>([]);
  const [recipientId, setRecipientId] = useState('');
  const [body, setBody] = useState('');
  const [loading, setLoading] = useState(false);
  const [threadLoading, setThreadLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string>();
  const [reportingId, setReportingId] = useState<string>();

  useEffect(() => {
    if (!visible) return;
    setSelected(undefined);
    setMessages([]);
    setRecipientId(initialRecipientId ?? '');
    setBody('');
    setError(undefined);
    void refreshConversations();
  }, [initialRecipientId, visible]);

  const refreshConversations = async () => {
    setLoading(true);
    setError(undefined);
    try {
      const page = await getDirectConversations();
      setConversations(page.items);
    } catch {
      setError('暂时无法读取私信，请稍后重试。');
    } finally {
      setLoading(false);
    }
  };

  const openConversation = async (conversation: DirectConversation) => {
    setSelected(conversation);
    setRecipientId(conversation.peer_user_id);
    setThreadLoading(true);
    setError(undefined);
    try {
      const page = await getDirectMessages(conversation.id);
      setMessages(page.items);
      if (page.items.length) await markDirectConversationRead(conversation.id, page.items[page.items.length - 1].id);
      setConversations((current) => current.map((item) => item.id === conversation.id ? { ...item, unread_count: 0 } : item));
    } catch {
      setError('暂时无法读取这段对话。');
    } finally {
      setThreadLoading(false);
    }
  };

  const send = async () => {
    const recipient = recipientId.trim();
    const text = body.trim();
    if (!recipient || !text || sending) return;
    setSending(true);
    setError(undefined);
    try {
      const message = await sendDirectMessage(recipient, text);
      setMessages((current) => [...current, message]);
      setBody('');
      await refreshConversations();
    } catch {
      setError('发送失败。对方可能关闭了私信，或这条消息需要安全审核。');
    } finally {
      setSending(false);
    }
  };

  const report = (messageId: string) => {
    Alert.alert('举报这条私信', '我们会将消息交给安全审核，举报不会通知对方。', [
      { text: '取消', style: 'cancel' },
      { text: '举报', style: 'destructive', onPress: () => {
        setReportingId(messageId);
        void reportDirectMessage(messageId)
          .then(() => setError('已提交举报，感谢你帮助维护交流环境。'))
          .catch(() => setError('举报未提交，请稍后重试。'))
          .finally(() => setReportingId(undefined));
      } },
    ]);
  };

  const title = selected ? `与 ${selected.peer_user_id} 的对话` : '私信';
  const canSend = Boolean(recipientId.trim() && body.trim() && !sending);
  const sortedMessages = useMemo(() => [...messages].sort((left, right) => left.created_at.localeCompare(right.created_at)), [messages]);

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel={selected ? '返回私信列表' : '关闭私信'} hitSlop={10} onPress={() => selected ? setSelected(undefined) : onClose()} style={styles.iconButton}>{selected ? <ArrowLeft color={colors.ink} size={21} /> : <X color={colors.ink} size={21} />}</Pressable>
          <Text numberOfLines={1} style={styles.title}>{title}</Text>
          <Pressable accessibilityLabel="刷新私信" disabled={loading || Boolean(selected)} hitSlop={10} onPress={() => void refreshConversations()} style={styles.iconButton}>{loading ? <ActivityIndicator color={colors.evergreen} size="small" /> : <RefreshCw color={colors.evergreen} size={18} />}</Pressable>
        </View>
        {error ? <Text accessibilityLiveRegion="polite" style={styles.error}>{error}</Text> : null}
        {selected ? <View style={styles.thread}>
          {threadLoading ? <View style={styles.loading}><ActivityIndicator color={colors.evergreen} size="small" /><Text style={styles.muted}>正在读取对话…</Text></View> : <ScrollView contentContainerStyle={styles.messages} showsVerticalScrollIndicator={false}>{sortedMessages.map((message) => <View key={message.id} style={[styles.bubble, message.sender_user_id === recipientId ? styles.received : styles.sent]}><Text style={[styles.messageText, message.sender_user_id !== recipientId && styles.sentText]}>{message.body}</Text><View style={styles.messageMeta}><Text style={[styles.messageTime, message.sender_user_id !== recipientId && styles.sentTime]}>{formatTime(message.created_at)}</Text>{message.sender_user_id === recipientId ? <Pressable accessibilityLabel="举报这条私信" disabled={reportingId === message.id} hitSlop={6} onPress={() => report(message.id)}><Text style={styles.reportText}>{reportingId === message.id ? '提交中' : '举报'}</Text></Pressable> : null}</View></View>)}</ScrollView>}
          <View style={styles.composer}><TextInput accessibilityLabel="私信内容" editable={!sending} maxLength={2000} multiline onChangeText={setBody} placeholder="写一条友善的消息" placeholderTextColor={colors.faint} style={styles.input} value={body} /><Pressable accessibilityLabel="发送私信" disabled={!canSend} onPress={() => void send()} style={[styles.send, !canSend && styles.disabled]}><Send color={colors.surface} size={17} /></Pressable></View>
        </View> : <ScrollView contentContainerStyle={styles.list} showsVerticalScrollIndicator={false}>
          {initialRecipientId ? <View style={styles.newMessage}><Text style={styles.sectionTitle}>发起新对话</Text><Text style={styles.recipient}>对象：{initialRecipientId}</Text><TextInput accessibilityLabel="新私信内容" editable={!sending} maxLength={2000} multiline onChangeText={setBody} placeholder="写一条友善的消息" placeholderTextColor={colors.faint} style={styles.newInput} value={body} /><Pressable accessibilityLabel="发送新私信" disabled={!canSend} onPress={() => void send()} style={[styles.newSend, !canSend && styles.disabled]}><Send color={colors.surface} size={16} /><Text style={styles.newSendText}>{sending ? '发送中' : '发送'}</Text></Pressable></View> : null}
          {!loading && conversations.length === 0 && !initialRecipientId ? <View style={styles.empty}><Mail color={colors.evergreen} size={24} /><Text style={styles.emptyTitle}>还没有私信</Text><Text style={styles.muted}>从创作者主页发起一段具体、友善的交流。</Text></View> : null}
          {conversations.map((conversation) => <Pressable accessibilityLabel={`打开与 ${conversation.peer_user_id} 的对话`} key={conversation.id} onPress={() => void openConversation(conversation)} style={({ pressed }) => [styles.conversation, pressed && styles.pressed]}><View style={styles.avatar}><Text style={styles.avatarText}>{conversation.peer_user_id.slice(0, 1).toUpperCase()}</Text></View><View style={styles.conversationCopy}><Text style={styles.peer}>{conversation.peer_user_id}</Text><Text numberOfLines={1} style={styles.preview}>{conversation.last_message_preview}</Text></View>{conversation.unread_count ? <Text style={styles.unread}>{conversation.unread_count > 99 ? '99+' : conversation.unread_count}</Text> : null}</Pressable>)}
        </ScrollView>}
      </View>
    </Modal>
  );
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '' : new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(date);
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 14, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  iconButton: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  error: { paddingHorizontal: 18, paddingVertical: 9, color: colors.coral, fontSize: 12, lineHeight: 18, backgroundColor: colors.coralSoft, letterSpacing: 0 },
  list: { padding: 16, gap: 8 },
  conversation: { minHeight: 70, padding: 12, flexDirection: 'row', alignItems: 'center', gap: 11, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  avatar: { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  avatarText: { color: colors.evergreen, fontWeight: '800', fontSize: 14, letterSpacing: 0 },
  conversationCopy: { flex: 1, minWidth: 0, gap: 3 },
  peer: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  preview: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
  unread: { minWidth: 22, paddingHorizontal: 5, paddingVertical: 2, color: colors.surface, fontSize: 10, fontWeight: '700', textAlign: 'center', borderRadius: 10, backgroundColor: colors.evergreen, overflow: 'hidden', letterSpacing: 0 },
  empty: { minHeight: 260, alignItems: 'center', justifyContent: 'center', gap: 9 },
  emptyTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  muted: { color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  newMessage: { padding: 14, gap: 9, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  sectionTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  recipient: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  newInput: { minHeight: 76, padding: 10, color: colors.ink, fontSize: 13, lineHeight: 19, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background, letterSpacing: 0 },
  newSend: { minHeight: 38, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 6, borderRadius: 6, backgroundColor: colors.evergreen },
  newSendText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  thread: { flex: 1 },
  messages: { flexGrow: 1, padding: 16, gap: 9, justifyContent: 'flex-end' },
  bubble: { maxWidth: '82%', paddingHorizontal: 12, paddingVertical: 9, borderRadius: 8, gap: 3 },
  received: { alignSelf: 'flex-start', backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  sent: { alignSelf: 'flex-end', backgroundColor: colors.evergreen },
  messageText: { color: colors.ink, fontSize: 13, lineHeight: 19, letterSpacing: 0 },
  sentText: { color: colors.surface },
  messageTime: { color: colors.faint, fontSize: 9, letterSpacing: 0 },
  sentTime: { color: '#CDE3D7', textAlign: 'right' },
  messageMeta: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  reportText: { color: colors.coral, fontSize: 9, fontWeight: '700', letterSpacing: 0 },
  composer: { padding: 10, flexDirection: 'row', alignItems: 'flex-end', gap: 8, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line, backgroundColor: colors.surface },
  input: { flex: 1, maxHeight: 110, minHeight: 40, paddingHorizontal: 11, paddingVertical: 9, color: colors.ink, fontSize: 13, lineHeight: 19, borderRadius: 6, borderWidth: 1, borderColor: colors.line, letterSpacing: 0 },
  send: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center', borderRadius: 20, backgroundColor: colors.evergreen },
  disabled: { opacity: 0.45 },
  loading: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 9 },
  pressed: { opacity: 0.62 },
});
