import { BellRing, CheckCheck, X } from 'lucide-react-native';
import { Modal, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';

type Props = { visible: boolean; onClose: () => void };

const notifications = [
  { id: 'review', title: '本周回望已准备好', text: '看看哪些行动为你带来了真实变化。', time: '今天' },
  { id: 'route', title: '路线提醒', text: '今天还有一项行动等待你决定。', time: '今天' },
  { id: 'community', title: '新的同行回应', text: '有人收藏了你的路线经验。', time: '昨天' },
];

export function NotificationsModal({ visible, onClose }: Props) {
  return <Modal animationType="slide" onRequestClose={onClose} visible={visible}><View style={styles.screen}><View style={styles.header}><Pressable accessibilityLabel="关闭通知" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable><Text style={styles.title}>通知与提醒</Text><Pressable accessibilityLabel="全部标为已读" hitSlop={10} onPress={onClose} style={styles.close}><CheckCheck color={colors.evergreen} size={20} /></Pressable></View><ScrollView contentContainerStyle={styles.content}>{notifications.map((item) => <Pressable key={item.id} onPress={onClose} style={({ pressed }) => [styles.item, pressed && styles.pressed]}><View style={styles.icon}><BellRing color={colors.evergreen} size={17} /></View><View style={styles.copy}><View style={styles.itemTop}><Text style={styles.itemTitle}>{item.title}</Text><Text style={styles.time}>{item.time}</Text></View><Text style={styles.text}>{item.text}</Text></View></Pressable>)}</ScrollView></View></Modal>;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  content: { padding: 16, gap: 8 },
  item: { minHeight: 74, padding: 13, flexDirection: 'row', alignItems: 'center', gap: 11, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  icon: { width: 34, height: 34, alignItems: 'center', justifyContent: 'center', borderRadius: 7, backgroundColor: colors.evergreenSoft },
  copy: { flex: 1, minWidth: 0 },
  itemTop: { flexDirection: 'row', justifyContent: 'space-between', gap: 8 },
  itemTitle: { flex: 1, color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  time: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  text: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 3, letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
