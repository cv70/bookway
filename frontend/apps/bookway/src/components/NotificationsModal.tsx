import { BellRing, RefreshCw, X } from 'lucide-react-native';
import { ActivityIndicator, Modal, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';
import { UserNotification } from '../types';

type Props = {
  visible: boolean;
  notifications: UserNotification[];
  unreadCount: number;
  loading: boolean;
  loadingMore: boolean;
  nextCursor?: string | null;
  openingNotificationId?: string;
  failedNotificationId?: string;
  onClose: () => void;
  onRefresh: () => void;
  onLoadMore: () => void;
  onOpenNotification: (notification: UserNotification) => void;
};

export function NotificationsModal({ visible, notifications, unreadCount, loading, loadingMore, nextCursor, openingNotificationId, failedNotificationId, onClose, onRefresh, onLoadMore, onOpenNotification }: Props) {
  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭通知" hitSlop={10} onPress={onClose} style={styles.close}>
            <X color={colors.ink} size={22} />
          </Pressable>
          <View style={styles.titleGroup}>
            <Text style={styles.title}>通知与提醒</Text>
            <Text style={styles.subtitle}>{unreadCount ? `${unreadCount} 条未读` : '全部已读'}</Text>
          </View>
          <Pressable accessibilityLabel="刷新通知" disabled={loading} hitSlop={10} onPress={onRefresh} style={styles.close}>
            {loading ? <ActivityIndicator color={colors.evergreen} size="small" /> : <RefreshCw color={colors.evergreen} size={19} />}
          </Pressable>
        </View>
        <ScrollView
          contentContainerStyle={styles.content}
          onScroll={({ nativeEvent }) => {
            if (nativeEvent.layoutMeasurement.height + nativeEvent.contentOffset.y >= nativeEvent.contentSize.height - 160) onLoadMore();
          }}
          scrollEventThrottle={200}
        >
          {notifications.length ? notifications.map((item) => {
            const opening = item.id === openingNotificationId;
            const failed = item.id === failedNotificationId;
            return (
              <Pressable
                accessibilityHint={failed ? '关联内容暂不可查看，点击重试' : item.read_at ? '打开通知关联内容' : '打开通知并标记为已读'}
                accessibilityState={{ busy: opening }}
                disabled={opening}
                key={item.id}
                onPress={() => onOpenNotification(item)}
                style={({ pressed }) => [styles.item, !item.read_at && styles.unread, opening && styles.opening, pressed && styles.pressed]}
              >
                <View style={styles.icon}>{opening ? <ActivityIndicator color={colors.evergreen} size="small" /> : <BellRing color={item.kind === 'community' ? colors.blue : colors.evergreen} size={17} />}</View>
                <View style={styles.copy}>
                  <View style={styles.itemTop}>
                    <Text numberOfLines={1} style={styles.itemTitle}>{item.title}</Text>
                    <Text style={styles.time}>{formatNotificationTime(item.created_at)}</Text>
                  </View>
                  <Text style={styles.text}>{item.body}</Text>
                  {opening ? <Text accessibilityLiveRegion="polite" style={styles.loadingLabel}>正在打开关联内容…</Text> : failed ? <Text accessibilityLiveRegion="polite" style={styles.errorLabel}>关联内容暂不可查看，点击重试</Text> : !item.read_at ? <Text style={styles.unreadLabel}>未读</Text> : null}
                </View>
              </Pressable>
            );
          }) : <View style={styles.empty}><BellRing color={colors.evergreen} size={22} /><Text style={styles.emptyTitle}>此刻没有新的提醒</Text><Text style={styles.emptyText}>万卷行会保持安静。想继续时，再为自己选一个最小行动。</Text></View>}
          {nextCursor ? <Pressable accessibilityRole="button" disabled={loadingMore} onPress={onLoadMore} style={[styles.loadMore, loadingMore && styles.loadMoreDisabled]}>{loadingMore ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更早的通知</Text>}</Pressable> : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

function formatNotificationTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(date);
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  titleGroup: { flex: 1, alignItems: 'center' },
  title: { color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  subtitle: { color: colors.faint, fontSize: 10, marginTop: 1, letterSpacing: 0 },
  content: { padding: 16, gap: 8 },
  item: { minHeight: 74, padding: 13, flexDirection: 'row', alignItems: 'center', gap: 11, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  unread: { borderColor: '#B7D1C5', backgroundColor: '#FBFDFC' },
  opening: { opacity: 0.72 },
  icon: { width: 34, height: 34, alignItems: 'center', justifyContent: 'center', borderRadius: 7, backgroundColor: colors.evergreenSoft },
  copy: { flex: 1, minWidth: 0 },
  itemTop: { flexDirection: 'row', justifyContent: 'space-between', gap: 8 },
  itemTitle: { flex: 1, color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  time: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  text: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 3, letterSpacing: 0 },
  unreadLabel: { color: colors.evergreen, fontSize: 10, fontWeight: '700', marginTop: 5, letterSpacing: 0 },
  loadingLabel: { color: colors.muted, fontSize: 10, fontWeight: '600', marginTop: 5, letterSpacing: 0 },
  errorLabel: { color: colors.coral, fontSize: 10, fontWeight: '600', marginTop: 5, letterSpacing: 0 },
  empty: { minHeight: 230, paddingHorizontal: 26, alignItems: 'center', justifyContent: 'center', gap: 9 },
  emptyTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  emptyText: { color: colors.muted, fontSize: 12, lineHeight: 19, textAlign: 'center', letterSpacing: 0 },
  loadMore: { minHeight: 42, marginTop: 5, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  loadMoreDisabled: { opacity: 0.62 },
  loadMoreText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
