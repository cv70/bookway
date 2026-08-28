import { Award, BookOpenText, BookMarked, ChevronRight, FilePenLine, Globe2, LockKeyhole, Mail, MessageSquarePlus, Settings, ShieldCheck, Sparkles, UserRound } from 'lucide-react-native';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';
import { AccountProfile, GrowthEntry, Journey, MallOrder, Today } from '../types';

// Mirrors mall-order's MallOrderStatus enum.
const ORDER_STATUS: Record<number, { label: string; tone: string }> = {
  0: { label: '待支付', tone: colors.gold },
  1: { label: '支付确认中', tone: colors.gold },
  2: { label: '已支付', tone: colors.evergreen },
  3: { label: '已取消', tone: colors.faint },
  4: { label: '已过期', tone: colors.faint },
};

const orderTitle = (order: MallOrder) =>
  order.items.map((item) => `${item.title} ×${item.quantity}`).join('、');

const links = [
  { key: 'review', label: '成长回望', icon: Sparkles },
  { key: 'saved', label: '收藏与加入', icon: BookMarked },
  { key: 'creation', label: '创作中心', icon: FilePenLine },
  { key: 'archive', label: '成长档案', icon: Award },
  { key: 'privacy', label: '隐私与权限', icon: LockKeyhole },
  { key: 'settings', label: '设置与数据', icon: Settings },
] as const;

export type ProfileSection = typeof links[number]['key'];

export function ProfileScreen({ profile, journeys, today, entries, orders, moderator, offline = false, loading = false, journeysError = false, todayError = false, entriesError = false, ordersLoading = false, ordersError = false, onOpenSection, onOpenLibrary, onOpenPublicResources, onOpenFeedback, onOpenMessages, onOpenModeration, onRetryOrders }: { profile: AccountProfile; journeys: Journey[]; today: Today; entries: GrowthEntry[]; orders: MallOrder[]; moderator: boolean; offline?: boolean; loading?: boolean; journeysError?: boolean; todayError?: boolean; entriesError?: boolean; ordersLoading?: boolean; ordersError?: boolean; onOpenSection: (section: ProfileSection) => void; onOpenLibrary: () => void; onOpenPublicResources: () => void; onOpenFeedback: () => void; onOpenMessages: () => void; onOpenModeration: () => void; onRetryOrders?: () => void }) {
  const activeJourneys = journeys.filter((journey) => journey.status === 'active').length;
  // A stat whose fetch failed must not quietly read as a server-confirmed 0.
  const statValue = (failed: boolean, value: string) => (failed ? '未连接' : value);
  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader title="我的" />
      {offline ? <Text style={styles.offline}>未连接：暂时无法获取服务端数据，连接恢复后会自动更新</Text> : null}
      <View style={styles.identity}>
        <View style={styles.avatar}><UserRound color={colors.evergreen} size={34} /></View>
        <View style={styles.identityCopy}>
          <Text style={styles.name}>{profile.display_name}</Text>
          <Text style={styles.caption}>{profile.bio || '在万卷与山河之间，成为自己'}</Text>
        </View>
      </View>
      <View style={styles.stats}>
        <ProfileStat label="进行中路线" value={loading ? '—' : statValue(journeysError, String(activeJourneys))} />
        <ProfileStat label="今日完成" value={loading ? '—' : statValue(todayError, String(today.completed))} />
        <ProfileStat label="留下记录" value={loading ? '—' : statValue(entriesError, String(entries.length))} />
      </View>
      {orders.length > 0 ? (
        <View style={mergedStyles.ordersBlock}>
          <Text style={styles.sectionTitle}>我的订单</Text>
          {orders.slice(0, 5).map((order) => {
            const status = ORDER_STATUS[order.status] ?? { label: '未知状态', tone: colors.faint };
            return (
              <View key={order.id} style={mergedStyles.orderRow}>
                <View style={mergedStyles.orderCopy}>
                  <Text style={mergedStyles.orderTitle} numberOfLines={1}>{orderTitle(order)}</Text>
                  <Text style={styles.caption}>{order.created_at.slice(0, 10)} · {(order.total_cents / 100).toFixed(2)} {order.currency}</Text>
                </View>
                <Text style={[mergedStyles.orderStatus, { color: status.tone }]}>{status.label}</Text>
              </View>
            );
          })}
        </View>
      ) : ordersError ? (
        <View style={mergedStyles.ordersBlock}>
          <Text style={styles.sectionTitle}>我的订单</Text>
          <View style={mergedStyles.ordersState}>
            <Text style={mergedStyles.ordersStateTitle}>未连接</Text>
            <Text style={mergedStyles.ordersStateText}>暂时无法获取订单，连接恢复后会自动更新。</Text>
            {onRetryOrders ? (
              <Pressable accessibilityRole="button" onPress={onRetryOrders} style={({ pressed }) => [mergedStyles.ordersRetry, pressed && styles.pressed]}>
                <Text style={mergedStyles.ordersRetryText}>重试</Text>
              </Pressable>
            ) : null}
          </View>
        </View>
      ) : ordersLoading ? (
        <View style={mergedStyles.ordersBlock}>
          <Text style={styles.sectionTitle}>我的订单</Text>
          <Text style={mergedStyles.ordersStateText}>正在加载订单…</Text>
        </View>
      ) : null}
      <Text style={styles.sectionTitle}>成长资产</Text>
      <View style={styles.links}>
        <Pressable onPress={onOpenLibrary} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><BookOpenText color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>我的书架</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        <Pressable accessibilityLabel="打开私信" onPress={onOpenMessages} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><Mail color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>私信</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        <Pressable accessibilityLabel="打开公共资源目录" onPress={onOpenPublicResources} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><Globe2 color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>公共资源</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        {links.map(({ key, label, icon: Icon }) => (
          <Pressable key={key} onPress={() => onOpenSection(key)} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
            <View style={styles.linkIcon}><Icon color={colors.evergreen} size={19} /></View>
            <Text style={styles.linkText}>{label}</Text>
            <ChevronRight color={colors.faint} size={18} />
          </Pressable>
        ))}
        <Pressable onPress={onOpenFeedback} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><MessageSquarePlus color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>意见反馈</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        {moderator ? <Pressable accessibilityLabel="打开评论审核工作台" onPress={onOpenModeration} style={({ pressed }) => [styles.link, styles.moderationLink, pressed && styles.pressed]}>
          <View style={[styles.linkIcon, styles.moderationIcon]}><ShieldCheck color={colors.gold} size={19} /></View>
          <View style={styles.linkCopy}><Text style={styles.linkText}>评论审核工作台</Text><Text style={styles.linkHint}>仅限受授权审核角色</Text></View>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable> : null}
      </View>
    </ScrollView>
  );
}

function ProfileStat({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.stat}>
      <Text style={styles.statValue}>{value}</Text>
      <Text style={styles.statLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 30 },
  offline: { marginHorizontal: 16, marginBottom: 12, paddingHorizontal: 12, paddingVertical: 9, color: colors.muted, backgroundColor: colors.goldSoft, borderRadius: 6, fontSize: 12, lineHeight: 17 },
  identity: { flexDirection: 'row', alignItems: 'center', gap: 14, paddingHorizontal: 20, paddingVertical: 16 },
  avatar: { width: 64, height: 64, borderRadius: 32, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  identityCopy: { flex: 1, minWidth: 0 },
  name: { color: colors.ink, fontSize: 20, fontWeight: '700', letterSpacing: 0 },
  caption: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  stats: { minHeight: 90, flexDirection: 'row', alignItems: 'center', marginTop: 10, backgroundColor: colors.ink },
  stat: { flex: 1, alignItems: 'center' },
  statValue: { color: colors.surface, fontSize: 19, fontWeight: '800', letterSpacing: 0 },
  statLabel: { color: '#BBC1BD', fontSize: 10, marginTop: 4, letterSpacing: 0 },
  sectionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', marginHorizontal: 20, marginTop: 26, marginBottom: 10, letterSpacing: 0 },
  links: { backgroundColor: colors.surface, borderTopColor: colors.line, borderBottomColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth, borderBottomWidth: StyleSheet.hairlineWidth },
  link: { minHeight: 58, marginLeft: 20, paddingRight: 20, flexDirection: 'row', alignItems: 'center', gap: 12, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  pressed: { opacity: 0.55 },
  linkIcon: { width: 30, height: 30, borderRadius: 6, backgroundColor: colors.evergreenSoft, alignItems: 'center', justifyContent: 'center' },
  moderationLink: { backgroundColor: '#FFFBF2' },
  moderationIcon: { backgroundColor: colors.goldSoft },
  linkCopy: { flex: 1, minWidth: 0 },
  linkText: { flex: 1, color: colors.ink, fontSize: 14, fontWeight: '600', letterSpacing: 0 },
  linkHint: { color: colors.faint, fontSize: 10, marginTop: 2, letterSpacing: 0 },
});

const orderStyles = StyleSheet.create({
  ordersBlock: { marginTop: 18 },
  ordersState: { paddingHorizontal: 20, gap: 6 },
  ordersStateTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  ordersStateText: { color: colors.muted, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  ordersRetry: { marginTop: 4, height: 34, paddingHorizontal: 14, alignItems: 'center', justifyContent: 'center', alignSelf: 'flex-start', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  ordersRetryText: { color: colors.evergreen, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  orderRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 10,
    gap: 12,
  },
  orderCopy: { flex: 1, gap: 2 },
  orderTitle: { fontSize: 14, color: colors.ink },
  orderStatus: { fontSize: 12 },
});
const mergedStyles = { ...styles, ...orderStyles };
